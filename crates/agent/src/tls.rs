//! Agent-side TLS. Builds a `tokio_rustls::TlsConnector` from the
//! configured trust + identity material. Used when the runner URL is
//! `circus+tls://` or `[agent.tls]` is set in config.

use std::{io::BufReader, sync::Arc};

use rustls::{ClientConfig, RootCertStore};
use tokio_rustls::TlsConnector;

use crate::config::TlsConfig;

/// Build a connector that trusts `ca_file` for the server and presents
/// `cert_file` / `key_file` as the client identity (mTLS).
///
/// # Errors
/// Returns the underlying IO/rustls error on missing files, malformed
/// PEM, or unsupported key types.
pub fn build_client_connector(cfg: &TlsConfig) -> anyhow::Result<TlsConnector> {
  let mut roots = RootCertStore::empty();
  let ca_bytes = std::fs::read(&cfg.ca_file)?;
  for cert in rustls_pemfile::certs(&mut BufReader::new(ca_bytes.as_slice())) {
    let cert = cert?;
    roots.add(cert)?;
  }

  let cert_bytes = std::fs::read(&cfg.cert_file)?;
  let cert_chain: Vec<_> =
    rustls_pemfile::certs(&mut BufReader::new(cert_bytes.as_slice()))
      .collect::<Result<_, _>>()?;

  let key_bytes = std::fs::read(&cfg.key_file)?;
  let key =
    rustls_pemfile::private_key(&mut BufReader::new(key_bytes.as_slice()))?
      .ok_or_else(|| {
        anyhow::anyhow!("no private key in {}", cfg.key_file.display())
      })?;

  let cfg = ClientConfig::builder()
    .with_root_certificates(roots)
    .with_client_auth_cert(cert_chain, key)?;
  Ok(TlsConnector::from(Arc::new(cfg)))
}
