//! LDAP bind authentication for the dashboard.
//!
//! Posts a `{username, password}` JSON body; we substitute the username
//! into the configured bind DN template and try a simple bind. On
//! success we upsert a local user record (so subsequent dashboard
//! actions have something to attach permissions to) and hand back a
//! session cookie identical to the OAuth flow.

use std::{io, path::Path, sync::Arc, time::Duration};

use axum::{
  Json,
  Router,
  extract::State,
  http::{StatusCode, header},
  response::{IntoResponse, Response},
  routing::post,
};
use circus_common::{models::UserType, repo};
use data_encoding::HEXLOWER;
use rustls::{
  ClientConfig,
  RootCertStore,
  pki_types::{CertificateDer, ServerName, pem::PemObject},
};
use serde::Deserialize;
use tokio::{
  io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
  net::TcpStream,
  time::timeout,
};
use tokio_rustls::TlsConnector;
use url::Url;

use crate::{
  error::ApiError,
  session_cookie::user_session_cookie,
  state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct LdapLoginRequest {
  pub username: String,
  pub password: String,
}

/// Bounds connect, TLS and the bind round trip together.
const LDAP_TIMEOUT: Duration = Duration::from_secs(10);
/// Bind responses are tiny; anything larger is not a well-behaved server.
const MAX_RESPONSE_LEN: usize = 64 * 1024;

/// RFC 4514 attribute value escaping, as hex pairs.
fn escape_ldap_value(input: &str) -> String {
  let mut out = String::with_capacity(input.len());

  for (index, ch) in input.char_indices() {
    let escape =
      matches!(ch, '"' | '+' | ',' | ';' | '<' | '=' | '>' | '\\' | '\0')
        || (index == 0 && matches!(ch, ' ' | '#'))
        || (index + ch.len_utf8() == input.len() && ch == ' ');

    if escape {
      out.push('\\');
      out.push_str(&HEXLOWER.encode(&[ch as u8]));
    } else {
      out.push(ch);
    }
  }

  out
}

fn tlv(tag: u8, value: &[u8]) -> Vec<u8> {
  let mut out = vec![tag];
  if let Ok(short) = u8::try_from(value.len())
    && short < 0x80
  {
    out.push(short);
  } else {
    let len = value.len().to_be_bytes();
    let skip = len.iter().take_while(|byte| **byte == 0).count();
    out.push(0x80 | (len.len() - skip) as u8);
    out.extend_from_slice(&len[skip..]);
  }
  out.extend_from_slice(value);
  out
}

/// `LDAPMessage { messageID 1, bindRequest { version 3, name, simple } }`.
fn bind_request(dn: &str, password: &str) -> Vec<u8> {
  let bind = [
    tlv(0x02, &[3]),
    tlv(0x04, dn.as_bytes()),
    tlv(0x80, password.as_bytes()),
  ]
  .concat();
  tlv(0x30, &[tlv(0x02, &[1]), tlv(0x60, &bind)].concat())
}

/// `LDAPMessage { messageID 2, unbindRequest }`.
const UNBIND_REQUEST: [u8; 7] = [0x30, 0x05, 0x02, 0x01, 0x02, 0x42, 0x00];

fn split_tlv(input: &[u8]) -> Option<(u8, &[u8], &[u8])> {
  let (&tag, rest) = input.split_first()?;
  let (&first, rest) = rest.split_first()?;
  let (len, rest) = if first & 0x80 == 0 {
    (usize::from(first), rest)
  } else {
    let count = usize::from(first & 0x7F);
    if count == 0 || count > 4 || rest.len() < count {
      return None;
    }
    let (len, rest) = rest.split_at(count);
    (
      len
        .iter()
        .fold(0, |acc, byte| acc << 8 | usize::from(*byte)),
      rest,
    )
  };
  (rest.len() >= len).then(|| (tag, &rest[..len], &rest[len..]))
}

async fn read_message(
  stream: &mut (impl AsyncRead + Unpin),
) -> io::Result<Vec<u8>> {
  let invalid =
    || io::Error::new(io::ErrorKind::InvalidData, "malformed LDAP response");
  let mut header = [0u8; 2];
  stream.read_exact(&mut header).await?;

  let mut message = header.to_vec();
  let len = if header[1] & 0x80 == 0 {
    usize::from(header[1])
  } else {
    let count = usize::from(header[1] & 0x7F);
    if count == 0 || count > 4 {
      return Err(invalid());
    }
    let mut len = vec![0u8; count];
    stream.read_exact(&mut len).await?;
    message.extend_from_slice(&len);
    len
      .iter()
      .fold(0, |acc, byte| acc << 8 | usize::from(*byte))
  };
  if len > MAX_RESPONSE_LEN {
    return Err(invalid());
  }

  let start = message.len();
  message.resize(start + len, 0);
  stream.read_exact(&mut message[start..]).await?;
  Ok(message)
}

/// The `resultCode` of a `bindResponse`.
fn bind_result_code(message: &[u8]) -> Option<u32> {
  let (0x30, body, _) = split_tlv(message)? else {
    return None;
  };
  let (0x02, _, body) = split_tlv(body)? else {
    return None;
  };
  let (0x61, response, _) = split_tlv(body)? else {
    return None;
  };
  let (0x0A, code, _) = split_tlv(response)? else {
    return None;
  };
  (1..=4)
    .contains(&code.len())
    .then(|| code.iter().fold(0, |acc, byte| acc << 8 | u32::from(*byte)))
}

async fn exchange(
  mut stream: impl AsyncRead + AsyncWrite + Unpin,
  request: &[u8],
) -> io::Result<u32> {
  stream.write_all(request).await?;
  let response = read_message(&mut stream).await?;
  if let Err(e) = stream.write_all(&UNBIND_REQUEST).await {
    tracing::warn!("LDAP unbind failed: {e}");
  }

  bind_result_code(&response).ok_or_else(|| {
    io::Error::new(io::ErrorKind::InvalidData, "malformed LDAP bind response")
  })
}

fn tls_connector(ca_cert: Option<&Path>) -> io::Result<TlsConnector> {
  let mut roots = RootCertStore::empty();
  roots
    .add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
  if let Some(path) = ca_cert {
    for cert in CertificateDer::pem_file_iter(path).map_err(io::Error::other)? {
      roots
        .add(cert.map_err(io::Error::other)?)
        .map_err(io::Error::other)?;
    }
  }

  let config = ClientConfig::builder()
    .with_root_certificates(roots)
    .with_no_client_auth();
  Ok(TlsConnector::from(Arc::new(config)))
}

enum BindError {
  /// The server could not be reached or spoke no TLS.
  Connect(io::Error),
  /// The server answered with something that is not a bind response.
  Protocol(io::Error),
}

/// Simple bind as `dn`, returning the server's result code.
async fn simple_bind(
  url: &str,
  ca_cert: Option<&Path>,
  dn: &str,
  password: &str,
) -> Result<u32, BindError> {
  let invalid = |what: &str| {
    BindError::Connect(io::Error::new(
      io::ErrorKind::InvalidInput,
      what.to_owned(),
    ))
  };
  let url = Url::parse(url).map_err(|_| invalid("LDAP url is not a URL"))?;
  let host = url
    .host_str()
    .ok_or_else(|| invalid("LDAP url has no host"))?
    .to_owned();
  let (tls, default_port) = match url.scheme() {
    "ldap" => (false, 389),
    "ldaps" => (true, 636),
    _ => return Err(invalid("LDAP url must use ldap:// or ldaps://")),
  };
  let port = url.port().unwrap_or(default_port);
  let request = bind_request(dn, password);

  let bind = async {
    let tcp = TcpStream::connect((host.as_str(), port))
      .await
      .map_err(BindError::Connect)?;

    if !tls {
      return exchange(tcp, &request).await.map_err(BindError::Protocol);
    }

    let name = ServerName::try_from(host.clone())
      .map_err(|e| BindError::Connect(io::Error::other(e)))?;
    let stream = tls_connector(ca_cert)
      .map_err(BindError::Connect)?
      .connect(name, tcp)
      .await
      .map_err(BindError::Connect)?;
    exchange(stream, &request)
      .await
      .map_err(BindError::Protocol)
  };

  timeout(LDAP_TIMEOUT, bind)
    .await
    .map_err(|_| BindError::Connect(io::ErrorKind::TimedOut.into()))?
}

async fn ldap_login(
  State(state): State<AppState>,
  Json(body): Json<LdapLoginRequest>,
) -> Result<Response, ApiError> {
  if body.username.trim().is_empty() || body.password.is_empty() {
    return Ok(StatusCode::BAD_REQUEST.into_response());
  }

  let Some(ldap_cfg) = state.config.server.ldap.as_ref() else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };
  if !ldap_cfg.enabled {
    return Ok(StatusCode::NOT_FOUND.into_response());
  }

  let escaped = escape_ldap_value(&body.username);
  let bind_dn = ldap_cfg.bind_dn_template.replace("{username}", &escaped);

  let bind = simple_bind(
    &ldap_cfg.url,
    ldap_cfg.tls_ca_cert.as_deref(),
    &bind_dn,
    &body.password,
  )
  .await;

  let reason = match bind {
    Ok(0) => None,
    Ok(rc) => {
      tracing::debug!(rc, "LDAP bind rejected");
      Some(serde_json::json!({ "reason": "bind_rejected", "rc": rc }))
    },
    Err(BindError::Connect(e)) => {
      tracing::debug!("LDAP connect failed: {e}");
      return Ok(StatusCode::SERVICE_UNAVAILABLE.into_response());
    },
    Err(BindError::Protocol(e)) => {
      tracing::debug!("LDAP bind error: {e}");
      Some(serde_json::json!({ "reason": "bind_error" }))
    },
  };

  if let Some(reason) = reason {
    crate::audit::record_with_actor(
      &state.pool,
      &circus_common::audit::Actor::anonymous(),
      None,
      "LOGIN_FAILURE",
      Some("ldap"),
      Some(&body.username),
      reason,
    )
    .await;
    return Ok(StatusCode::UNAUTHORIZED.into_response());
  }

  // Treat the username as the LDAP-side identity for upsert. This mirrors
  // OAuth's "use the provider id to disambiguate users with the same login".
  let user = repo::users::upsert_oauth_user(
    &state.pool,
    &body.username,
    None,
    UserType::Ldap,
    &body.username,
    state.email_regex.as_deref(),
  )
  .await?;

  let session = repo::users::create_session(&state.pool, user.id).await?;

  crate::audit::record_with_actor(
    &state.pool,
    &circus_common::audit::Actor::user(user.id, user.username.clone()),
    None,
    "LOGIN_SUCCESS",
    Some("ldap"),
    Some(&user.id.to_string()),
    serde_json::json!({ "method": "ldap" }),
  )
  .await;

  let cookie = user_session_cookie(&session.0, &state.config.server);

  Ok(
    Response::builder()
      .status(StatusCode::NO_CONTENT)
      .header(header::SET_COOKIE, cookie)
      .body(axum::body::Body::empty())
      .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response()),
  )
}

pub fn router() -> Router<AppState> {
  Router::new().route("/auth/ldap", post(ldap_login))
}
