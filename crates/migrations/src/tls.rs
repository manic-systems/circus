//! TLS configuration shared by migrations and the application pool.

use std::{
  convert::Infallible,
  future::Future,
  io,
  net::IpAddr,
  path::PathBuf,
  pin::Pin,
  sync::{Arc, Once},
  task::{Context, Poll},
};

use misstep::{ResultExt as _, bail};
use rustls::{
  ClientConfig,
  DigitallySignedStruct,
  SignatureScheme,
  client::danger::{
    HandshakeSignatureValid,
    ServerCertVerified,
    ServerCertVerifier,
  },
  crypto::WebPkiSupportedAlgorithms,
  pki_types::{CertificateDer, ServerName, UnixTime, pem::PemObject as _},
  server::ParsedCertificate,
};
use sha2::{Digest as _, Sha256, Sha384, Sha512};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_postgres::{
  NoTls,
  tls::{ChannelBinding, MakeTlsConnect, TlsConnect},
};
use tokio_rustls::{TlsConnector, client::TlsStream};
use url::Url;
use x509_cert::{
  Certificate,
  der::{
    Decode as _,
    oid::db::rfc5912::{
      ECDSA_WITH_SHA_256,
      ECDSA_WITH_SHA_384,
      ECDSA_WITH_SHA_512,
      SHA_1_WITH_RSA_ENCRYPTION,
      SHA_256_WITH_RSA_ENCRYPTION,
      SHA_384_WITH_RSA_ENCRYPTION,
      SHA_512_WITH_RSA_ENCRYPTION,
    },
  },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TlsMode {
  Disable,
  Unverified,
  VerifyCa,
  VerifyFull,
}

/// `tls` is `None` for plaintext.
pub struct DatabaseTarget {
  pub url: String,
  pub tls: Option<DatabaseTls>,
}

/// Without `sslmode`, remote hosts default to `verify-full`.
///
/// # Errors
///
/// Returns an error for a non-URL connection string, an unknown `sslmode`,
/// or an unreadable `sslrootcert`.
pub fn resolve(database_url: &str) -> misstep::Result<DatabaseTarget> {
  let mut url = Url::parse(database_url)
    .ok()
    .filter(|url| matches!(url.scheme(), "postgres" | "postgresql"))
    .ok_or_else(|| {
      misstep::report!("database URL must be a postgresql:// URL")
    })?;

  let mut sslmode = None;
  let mut root_cert = None;
  let mut hosts: Vec<String> = url
    .host_str()
    .filter(|host| !host.is_empty())
    .map(str::to_owned)
    .into_iter()
    .collect();
  let mut pairs = Vec::new();
  for (key, value) in url.query_pairs() {
    match key.as_ref() {
      "sslmode" => sslmode = Some(value.to_ascii_lowercase()),
      "sslrootcert" => root_cert = Some(PathBuf::from(value.as_ref())),
      _ => {
        if key == "host" {
          hosts.extend(value.split(',').map(str::to_owned));
        }
        pairs.push((key.into_owned(), value.into_owned()));
      },
    }
  }

  let mode = match sslmode.as_deref() {
    None if hosts.iter().all(|host| is_local_host(host)) => TlsMode::Disable,
    None | Some("verify-full") => TlsMode::VerifyFull,
    Some("disable") => TlsMode::Disable,
    Some("allow" | "prefer" | "require") => TlsMode::Unverified,
    Some("verify-ca") => TlsMode::VerifyCa,
    Some(other) => bail!("unsupported database sslmode '{other}'"),
  };
  let tokio_sslmode = match (mode, sslmode.as_deref()) {
    (TlsMode::Disable, _) => "disable",
    (TlsMode::Unverified, Some("allow" | "prefer")) => "prefer",
    _ => "require",
  };
  pairs.push(("sslmode".to_owned(), tokio_sslmode.to_owned()));
  url.query_pairs_mut().clear().extend_pairs(&pairs);

  let tls = match mode {
    TlsMode::Disable => None,
    mode => Some(tls_connector(mode, root_store(root_cert.as_ref())?)),
  };
  Ok(DatabaseTarget {
    url: url.into(),
    tls,
  })
}

fn is_local_host(host: &str) -> bool {
  host.starts_with('/')
    || host.starts_with('@')
    || host.eq_ignore_ascii_case("localhost")
    || host
      .trim_matches(['[', ']'])
      .parse::<IpAddr>()
      .is_ok_and(|ip| ip.is_loopback())
}

fn tls_connector(mode: TlsMode, roots: rustls::RootCertStore) -> DatabaseTls {
  static TLS_PROVIDER: Once = Once::new();

  let provider = rustls::crypto::ring::default_provider();
  let signature_algorithms = provider.signature_verification_algorithms;
  TLS_PROVIDER.call_once(move || {
    let _ = provider.install_default();
  });

  let builder = rustls::ClientConfig::builder();
  let config = match mode {
    TlsMode::VerifyFull => {
      builder.with_root_certificates(roots).with_no_client_auth()
    },
    TlsMode::VerifyCa => {
      builder
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(CaOnlyVerifier {
          roots,
          signature_algorithms,
        }))
        .with_no_client_auth()
    },
    TlsMode::Disable | TlsMode::Unverified => {
      builder
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoCertificateVerification {
          signature_algorithms,
        }))
        .with_no_client_auth()
    },
  };
  DatabaseTls(Arc::new(config))
}

fn root_store(
  root_cert: Option<&PathBuf>,
) -> misstep::Result<rustls::RootCertStore> {
  let mut roots = rustls::RootCertStore::empty();
  let Some(path) = root_cert else {
    let native = rustls_native_certs::load_native_certs();

    for error in &native.errors {
      tracing::warn!(%error, "skipping unparseable system certificate");
    }

    roots.add_parsable_certificates(native.certs);
    return Ok(roots);
  };
  for cert in CertificateDer::pem_file_iter(path)
    .with_context(|| format!("reading sslrootcert {}", path.display()))?
  {
    let cert = cert
      .with_context(|| format!("parsing sslrootcert {}", path.display()))?;
    roots
      .add(cert)
      .with_context(|| format!("loading sslrootcert {}", path.display()))?;
  }
  Ok(roots)
}

/// Connect a client and drive its connection on a background task.
///
/// # Errors
///
/// Returns an error when URL resolution, negotiation, or startup fails.
pub async fn connect_once(
  database_url: &str,
) -> misstep::Result<tokio_postgres::Client> {
  let target = resolve(database_url)?;
  let config = target.url.parse::<tokio_postgres::Config>()?;
  let client = match target.tls {
    None => {
      let (client, connection) = config.connect(NoTls).await?;
      spawn_connection(connection);
      client
    },
    Some(connector) => {
      let (client, connection) = config.connect(connector).await?;
      spawn_connection(connection);
      client
    },
  };
  Ok(client)
}

fn spawn_connection(
  connection: impl std::future::Future<
    Output = std::result::Result<(), tokio_postgres::Error>,
  > + Send
  + 'static,
) {
  tokio::spawn(async move {
    if let Err(err) = connection.await {
      tracing::error!(?err, "postgres connection task ended with error");
    }
  });
}

/// `tokio_postgres` TLS over rustls.
#[derive(Clone)]
pub struct DatabaseTls(Arc<ClientConfig>);

impl<S> MakeTlsConnect<S> for DatabaseTls
where
  S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
  type Error = Infallible;
  type Stream = DatabaseTlsStream<S>;
  type TlsConnect = DatabaseTlsHandshake;

  fn make_tls_connect(
    &mut self,
    hostname: &str,
  ) -> std::result::Result<DatabaseTlsHandshake, Infallible> {
    Ok(DatabaseTlsHandshake {
      hostname:  hostname.to_owned(),
      connector: TlsConnector::from(Arc::clone(&self.0)),
    })
  }
}

pub struct DatabaseTlsHandshake {
  hostname:  String,
  connector: TlsConnector,
}

impl<S> TlsConnect<S> for DatabaseTlsHandshake
where
  S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
  type Error = io::Error;
  type Future =
    Pin<Box<dyn Future<Output = io::Result<DatabaseTlsStream<S>>> + Send>>;
  type Stream = DatabaseTlsStream<S>;

  fn connect(self, stream: S) -> Self::Future {
    Box::pin(async move {
      let server_name = ServerName::try_from(self.hostname)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
      self
        .connector
        .connect(server_name, stream)
        .await
        .map(DatabaseTlsStream)
    })
  }
}

pub struct DatabaseTlsStream<S>(TlsStream<S>);

impl<S> tokio_postgres::tls::TlsStream for DatabaseTlsStream<S>
where
  S: AsyncRead + AsyncWrite + Unpin,
{
  /// RFC 5929 `tls-server-end-point` hashes the certificate with its
  /// signature digest, upgrading SHA-1 to SHA-256. Ed25519 and RSA-PSS have
  /// no fixed digest, so they get no binding.
  fn channel_binding(&self) -> ChannelBinding {
    let (_, session) = self.0.get_ref();
    let Some(der) = session.peer_certificates().and_then(<[_]>::first) else {
      return ChannelBinding::none();
    };
    let Ok(certificate) = Certificate::from_der(der) else {
      return ChannelBinding::none();
    };

    let digest = match certificate.signature_algorithm().oid {
      SHA_1_WITH_RSA_ENCRYPTION
      | SHA_256_WITH_RSA_ENCRYPTION
      | ECDSA_WITH_SHA_256 => Sha256::digest(der).to_vec(),
      SHA_384_WITH_RSA_ENCRYPTION | ECDSA_WITH_SHA_384 => {
        Sha384::digest(der).to_vec()
      },
      SHA_512_WITH_RSA_ENCRYPTION | ECDSA_WITH_SHA_512 => {
        Sha512::digest(der).to_vec()
      },
      _ => return ChannelBinding::none(),
    };
    ChannelBinding::tls_server_end_point(digest)
  }
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncRead for DatabaseTlsStream<S> {
  fn poll_read(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
  ) -> Poll<io::Result<()>> {
    Pin::new(&mut self.0).poll_read(cx, buf)
  }
}

impl<S: AsyncRead + AsyncWrite + Unpin> AsyncWrite for DatabaseTlsStream<S> {
  fn poll_write(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &[u8],
  ) -> Poll<io::Result<usize>> {
    Pin::new(&mut self.0).poll_write(cx, buf)
  }

  fn poll_flush(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<io::Result<()>> {
    Pin::new(&mut self.0).poll_flush(cx)
  }

  fn poll_shutdown(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<io::Result<()>> {
    Pin::new(&mut self.0).poll_shutdown(cx)
  }
}

/// Encrypted but unverified, matching what libpq does for `require`, `prefer`,
/// and `allow`.
#[derive(Debug)]
struct NoCertificateVerification {
  signature_algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for NoCertificateVerification {
  fn verify_server_cert(
    &self,
    _end_entity: &CertificateDer<'_>,
    _intermediates: &[CertificateDer<'_>],
    _server_name: &ServerName<'_>,
    _ocsp_response: &[u8],
    _now: UnixTime,
  ) -> std::result::Result<ServerCertVerified, rustls::Error> {
    Ok(ServerCertVerified::assertion())
  }

  fn verify_tls12_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
    rustls::crypto::verify_tls12_signature(
      message,
      cert,
      dss,
      &self.signature_algorithms,
    )
  }

  fn verify_tls13_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
    rustls::crypto::verify_tls13_signature(
      message,
      cert,
      dss,
      &self.signature_algorithms,
    )
  }

  fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
    self.signature_algorithms.supported_schemes()
  }
}

/// Chain validation without the hostname check, matching libpq's `verify-ca`.
#[derive(Debug)]
struct CaOnlyVerifier {
  roots:                rustls::RootCertStore,
  signature_algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for CaOnlyVerifier {
  fn verify_server_cert(
    &self,
    end_entity: &CertificateDer<'_>,
    intermediates: &[CertificateDer<'_>],
    _server_name: &ServerName<'_>,
    _ocsp_response: &[u8],
    now: UnixTime,
  ) -> std::result::Result<ServerCertVerified, rustls::Error> {
    let cert = ParsedCertificate::try_from(end_entity)?;
    rustls::client::verify_server_cert_signed_by_trust_anchor(
      &cert,
      &self.roots,
      intermediates,
      now,
      self.signature_algorithms.all,
    )?;
    Ok(ServerCertVerified::assertion())
  }

  fn verify_tls12_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
    rustls::crypto::verify_tls12_signature(
      message,
      cert,
      dss,
      &self.signature_algorithms,
    )
  }

  fn verify_tls13_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
    rustls::crypto::verify_tls13_signature(
      message,
      cert,
      dss,
      &self.signature_algorithms,
    )
  }

  fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
    self.signature_algorithms.supported_schemes()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn mode_of(url: &str) -> Option<&'static str> {
    let target = resolve(url).ok()?;
    let tls = target.tls.is_some();
    let config = target.url.parse::<tokio_postgres::Config>().ok()?;
    Some(match (tls, config.get_ssl_mode()) {
      (false, tokio_postgres::config::SslMode::Disable) => "plain",
      (true, tokio_postgres::config::SslMode::Require) => "required-tls",
      (true, tokio_postgres::config::SslMode::Prefer) => "optional-tls",
      _ => "inconsistent",
    })
  }

  #[test]
  fn remote_hosts_require_tls_unless_told_otherwise() {
    assert_eq!(
      mode_of("postgresql:///circus?host=/run/postgresql"),
      Some("plain")
    );
    assert_eq!(mode_of("postgresql://localhost/circus"), Some("plain"));
    assert_eq!(
      mode_of("postgresql://db.example/circus"),
      Some("required-tls")
    );
    assert_eq!(
      mode_of("postgresql://db.example/circus?sslmode=disable"),
      Some("plain")
    );
    assert_eq!(
      mode_of("postgresql://db.example/circus?sslmode=prefer"),
      Some("optional-tls")
    );
    assert_eq!(
      mode_of("postgresql://db.example/circus?sslmode=bogus"),
      None
    );
    assert_eq!(mode_of("host=db.example dbname=circus"), None);
  }
}
