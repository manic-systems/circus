//! TLS configuration shared by migrations and the application pool.

use std::{
  net::IpAddr,
  path::PathBuf,
  sync::{Arc, Once},
};

use color_eyre::eyre::{Context as _, bail};
use rustls::{
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
use tokio_postgres::NoTls;
use tokio_postgres_rustls::MakeRustlsConnect;
use url::Url;

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
  pub tls: Option<MakeRustlsConnect>,
}

/// Without `sslmode`, remote hosts default to `verify-full`.
///
/// # Errors
///
/// Returns an error for a non-URL connection string, an unknown `sslmode`,
/// or an unreadable `sslrootcert`.
pub fn resolve(database_url: &str) -> color_eyre::Result<DatabaseTarget> {
  let mut url = Url::parse(database_url)
    .ok()
    .filter(|url| matches!(url.scheme(), "postgres" | "postgresql"))
    .ok_or_else(|| {
      color_eyre::eyre::eyre!("database URL must be a postgresql:// URL")
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

fn tls_connector(
  mode: TlsMode,
  roots: rustls::RootCertStore,
) -> MakeRustlsConnect {
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
  MakeRustlsConnect::new(config)
}

fn root_store(
  root_cert: Option<&PathBuf>,
) -> color_eyre::Result<rustls::RootCertStore> {
  let Some(path) = root_cert else {
    return Ok(webpki_roots::TLS_SERVER_ROOTS.iter().cloned().collect());
  };
  let mut roots = rustls::RootCertStore::empty();
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
) -> color_eyre::Result<tokio_postgres::Client> {
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
