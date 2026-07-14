//! TLS configuration shared by migrations and the application pool.

use std::sync::{Arc, Once};

use rustls::{
  DigitallySignedStruct,
  SignatureScheme,
  client::danger::{
    HandshakeSignatureValid,
    ServerCertVerified,
    ServerCertVerifier,
  },
  crypto::WebPkiSupportedAlgorithms,
  pki_types::{CertificateDer, ServerName, UnixTime},
  server::ParsedCertificate,
};
use tokio_postgres::NoTls;
use tokio_postgres_rustls::MakeRustlsConnect;
use url::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlsMode {
  Disable,
  Unverified,
  VerifyCa,
  VerifyFull,
}

#[must_use]
pub fn tls_mode(database_url: &str) -> TlsMode {
  let Some(sslmode) = Url::parse(database_url)
    .ok()
    .and_then(|url| sslmode_from_query(url.query()))
  else {
    return TlsMode::Unverified;
  };

  match sslmode.to_ascii_lowercase().as_str() {
    "disable" => TlsMode::Disable,
    "verify-ca" => TlsMode::VerifyCa,
    "verify-full" => TlsMode::VerifyFull,
    _ => TlsMode::Unverified,
  }
}

fn sslmode_from_query(query: Option<&str>) -> Option<String> {
  url::form_urlencoded::parse(query?.as_bytes())
    .find(|(key, _)| key == "sslmode")
    .map(|(_, value)| value.into_owned())
}

/// Normalize libpq modes that tokio-postgres doesn't parse itself.
#[must_use]
pub fn tokio_postgres_url(database_url: &str) -> String {
  let Ok(mut url) = Url::parse(database_url) else {
    return database_url.to_owned();
  };

  let mut changed = false;
  let pairs: Vec<(String, String)> = url
    .query_pairs()
    .map(|(key, value)| {
      let normalized = if key == "sslmode" {
        match value.to_ascii_lowercase().as_str() {
          "allow" | "prefer" => "prefer",
          "verify-ca" | "verify-full" | "require" => "require",
          "disable" => "disable",
          _ => value.as_ref(),
        }
      } else {
        value.as_ref()
      };
      changed |= normalized != value;
      (key.into_owned(), normalized.to_owned())
    })
    .collect();

  if changed {
    url.query_pairs_mut().clear().extend_pairs(&pairs);
    url.into()
  } else {
    database_url.to_owned()
  }
}

#[must_use]
pub fn tls_connector(mode: TlsMode) -> MakeRustlsConnect {
  static TLS_PROVIDER: Once = Once::new();

  let provider = rustls::crypto::ring::default_provider();
  let signature_algorithms = provider.signature_verification_algorithms;
  TLS_PROVIDER.call_once(move || {
    let _ = provider.install_default();
  });

  let builder = rustls::ClientConfig::builder();
  let config = match mode {
    TlsMode::VerifyFull => {
      builder
        .with_root_certificates(root_store())
        .with_no_client_auth()
    },
    TlsMode::VerifyCa => {
      builder
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(CaOnlyVerifier {
          roots: root_store(),
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

fn root_store() -> rustls::RootCertStore {
  webpki_roots::TLS_SERVER_ROOTS.iter().cloned().collect()
}

/// Connect a client and drive its connection on a background task.
///
/// # Errors
///
/// Returns an error when URL parsing, negotiation, or startup fails.
pub async fn connect_once(
  database_url: &str,
) -> Result<tokio_postgres::Client, tokio_postgres::Error> {
  let config =
    tokio_postgres_url(database_url).parse::<tokio_postgres::Config>()?;
  match tls_mode(database_url) {
    TlsMode::Disable => {
      let (client, connection) = config.connect(NoTls).await?;
      spawn_connection(connection);
      Ok(client)
    },
    mode => {
      let (client, connection) = config.connect(tls_connector(mode)).await?;
      spawn_connection(connection);
      Ok(client)
    },
  }
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

  #[test]
  fn tls_mode_honors_postgres_sslmode() {
    assert_eq!(
      tls_mode("postgresql://localhost/circus"),
      TlsMode::Unverified
    );
    assert_eq!(
      tls_mode("postgresql://localhost/circus?sslmode=disable"),
      TlsMode::Disable
    );
    assert_eq!(
      tls_mode("postgresql://localhost/circus?sslmode=verify-ca"),
      TlsMode::VerifyCa
    );
    assert_eq!(
      tls_mode("postgresql://localhost/circus?sslmode=verify-full"),
      TlsMode::VerifyFull
    );
  }

  #[test]
  fn extended_ssl_modes_parse_as_tokio_postgres_urls() {
    for sslmode in ["allow", "verify-ca", "verify-full", "VERIFY-FULL"] {
      let url = tokio_postgres_url(&format!(
        "postgresql://localhost/circus?sslmode={sslmode}&\
         application_name=circus"
      ));
      assert!(url.parse::<tokio_postgres::Config>().is_ok(), "{url}");
      assert!(url.contains("application_name=circus"));
    }
  }
}
