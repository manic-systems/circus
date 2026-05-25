//! LDAP bind authentication for the dashboard.
//!
//! Posts a `{username, password}` JSON body; we substitute the username
//! into the configured bind DN template and try a simple bind. On
//! success we upsert a local user record (so subsequent dashboard
//! actions have something to attach permissions to) and hand back a
//! session cookie identical to the OAuth flow.

use axum::{
  Json,
  Router,
  extract::State,
  http::{StatusCode, header},
  response::{IntoResponse, Response},
  routing::post,
};
use fc_common::{models::UserType, repo};
use serde::Deserialize;

use crate::{error::ApiError, routes::cookie_security_flags, state::AppState};

#[derive(Debug, Deserialize)]
pub struct LdapLoginRequest {
  pub username: String,
  pub password: String,
}

/// LDAP DN/filter metacharacters that must be escaped before substituting
/// a username into a template. Without this an attacker could craft a
/// username like `admin)(uid=*` and alter the resulting DN.
fn escape_ldap_value(input: &str) -> String {
  let mut out = String::with_capacity(input.len());
  for ch in input.chars() {
    match ch {
      ',' | '\\' | '#' | '+' | '<' | '>' | ';' | '"' | '=' | '(' | ')'
      | '*' | '\0' => {
        out.push('\\');
        out.push(ch);
      },
      _ => out.push(ch),
    }
  }
  out
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

  let bind_result = ldap3::LdapConnAsync::new(&ldap_cfg.url).await;
  let (conn, mut ldap) = match bind_result {
    Ok(pair) => pair,
    Err(e) => {
      tracing::debug!("LDAP connect failed: {e}");
      return Ok(StatusCode::SERVICE_UNAVAILABLE.into_response());
    },
  };
  ldap3::drive!(conn);

  let bind = ldap.simple_bind(&bind_dn, &body.password).await;
  if let Err(e) = ldap.unbind().await {
    tracing::warn!("LDAP unbind failed: {e}");
  }

  match bind {
    Ok(res) if res.rc == 0 => {},
    Ok(res) => {
      tracing::debug!(rc = res.rc, "LDAP bind rejected");
      return Ok(StatusCode::UNAUTHORIZED.into_response());
    },
    Err(e) => {
      tracing::debug!("LDAP bind error: {e}");
      return Ok(StatusCode::UNAUTHORIZED.into_response());
    },
  }

  // Treat the username as the LDAP-side identity for upsert. This mirrors
  // OAuth's "use the provider id to disambiguate users with the same login".
  let user = repo::users::upsert_oauth_user(
    &state.pool,
    &body.username,
    None,
    UserType::Ldap,
    "ldap",
  )
  .await
  .map_err(ApiError)?;

  let session = repo::users::create_session(&state.pool, user.id)
    .await
    .map_err(ApiError)?;

  let cookie = format!(
    "fc_user_session={}; {}; Path=/; Max-Age={}",
    session.0,
    cookie_security_flags(&state.config.server),
    7 * 24 * 60 * 60
  );

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
