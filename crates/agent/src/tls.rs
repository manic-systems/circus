//! Agent-side TLS. Builds a `tokio_rustls::TlsConnector` from the
//! configured trust + identity material. Used when the runner URL is
//! `circus+tls://` or `[agent.tls]` is set in config.

use std::{io::BufReader, sync::Arc};

use color_eyre::eyre::{bail, eyre};
use rustls::{ClientConfig, RootCertStore, pki_types::CertificateDer};
use tokio_rustls::TlsConnector;

use crate::config::TlsConfig;

/// Build a connector that trusts `ca_file` or the OS root store for the server.
/// When `cert_file` and `key_file` are both set it also presents them as the
/// client identity for mTLS. When they're absent it connects with no client
/// auth, relying on the bearer token alone.
///
/// # Errors
///
/// Returns the underlying IO/rustls error on missing files, malformed
/// PEM, or unsupported key types.
pub fn build_client_connector(
  cfg: &TlsConfig,
) -> color_eyre::Result<TlsConnector> {
  let client_identity = match (&cfg.cert_file, &cfg.key_file) {
    (Some(cert_file), Some(key_file)) => Some((cert_file, key_file)),
    (None, None) => None,
    (Some(cert_file), None) => {
      bail!(
        "agent TLS cert_file {} requires key_file",
        cert_file.display()
      );
    },
    (None, Some(key_file)) => {
      bail!(
        "agent TLS key_file {} requires cert_file",
        key_file.display()
      );
    },
  };

  let mut roots = RootCertStore::empty();
  if let Some(ca_file) = &cfg.ca_file {
    let ca_bytes = std::fs::read(ca_file)?;
    for cert in rustls_pemfile::certs(&mut BufReader::new(ca_bytes.as_slice()))
    {
      roots.add(cert?)?;
    }
  } else {
    let native = rustls_native_certs::load_native_certs();
    for err in &native.errors {
      tracing::warn!(%err, "skipping unparseable system certificate");
    }
    if native.certs.is_empty() {
      bail!("no system CA certificates found; set tls.ca_file explicitly");
    }
    roots.add_parsable_certificates(native.certs);
  }

  let builder = ClientConfig::builder().with_root_certificates(roots);
  let client_cfg = if let Some((cert_file, key_file)) = client_identity {
    let cert_bytes = std::fs::read(cert_file)?;
    let cert_chain =
      rustls_pemfile::certs(&mut BufReader::new(cert_bytes.as_slice()))
        .collect::<Result<Vec<CertificateDer>, _>>()?;
    let key_bytes = std::fs::read(key_file)?;
    let key =
      rustls_pemfile::private_key(&mut BufReader::new(key_bytes.as_slice()))?
        .ok_or_else(|| eyre!("no private key in {}", key_file.display()))?;
    builder.with_client_auth_cert(cert_chain, key)?
  } else {
    builder.with_no_client_auth()
  };
  Ok(TlsConnector::from(Arc::new(client_cfg)))
}

#[cfg(test)]
#[expect(clippy::panic, reason = "it's in a test")]
mod tests {
  use std::path::PathBuf;

  use super::*;

  fn cfg(cert_file: Option<&str>, key_file: Option<&str>) -> TlsConfig {
    TlsConfig {
      ca_file:   Some(PathBuf::from("missing-ca.pem")),
      cert_file: cert_file.map(PathBuf::from),
      key_file:  key_file.map(PathBuf::from),
    }
  }

  #[test]
  fn rejects_cert_without_key() {
    let Err(err) = build_client_connector(&cfg(Some("agent.crt"), None)) else {
      panic!("partial client identity should fail");
    };
    assert!(err.to_string().contains("requires key_file"));
  }

  #[test]
  fn rejects_key_without_cert() {
    let Err(err) = build_client_connector(&cfg(None, Some("agent.key"))) else {
      panic!("partial client identity should fail");
    };
    assert!(err.to_string().contains("requires cert_file"));
  }
}
