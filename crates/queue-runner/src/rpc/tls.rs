//! Runner-side TLS. Builds a `tokio_rustls::TlsAcceptor` from the
//! configured server identity. When `client_ca` is set the acceptor
//! verifies any client cert an agent presents, and can require one when
//! `require_client_cert` is enabled. CN pinning runs after the handshake,
//! where it can compare the verified cert identity with the agent's registered
//! name.

use std::{io::BufReader, sync::Arc};

use circus_config::RpcTlsConfig;
use color_eyre::eyre::{Context as _, eyre};
use rustls::{RootCertStore, ServerConfig, server::WebPkiClientVerifier};
use tokio_rustls::TlsAcceptor;

/// Build an acceptor honoring `cfg`. Errors out on missing files or
/// malformed PEM; never panics.
///
/// # Errors
///
/// Returns any underlying IO or rustls error.
pub fn build_acceptor(cfg: &RpcTlsConfig) -> color_eyre::Result<TlsAcceptor> {
  let cert_bytes = std::fs::read(&cfg.cert_file)
    .with_context(|| format!("read cert {}", cfg.cert_file.display()))?;
  let cert_chain: Vec<_> =
    rustls_pemfile::certs(&mut BufReader::new(cert_bytes.as_slice()))
      .collect::<Result<_, _>>()?;

  let key_bytes = std::fs::read(&cfg.key_file)
    .with_context(|| format!("read key {}", cfg.key_file.display()))?;
  let key =
    rustls_pemfile::private_key(&mut BufReader::new(key_bytes.as_slice()))?
      .ok_or_else(|| eyre!("no private key in {}", cfg.key_file.display()))?;

  let server_cfg = if let Some(ca_path) = &cfg.client_ca {
    let ca_bytes = std::fs::read(ca_path)
      .with_context(|| format!("read client CA {}", ca_path.display()))?;
    let mut roots = RootCertStore::empty();
    for cert in rustls_pemfile::certs(&mut BufReader::new(ca_bytes.as_slice()))
    {
      roots.add(cert?)?;
    }
    let builder = WebPkiClientVerifier::builder(Arc::new(roots));
    let verifier = if cfg.require_client_cert {
      builder.build()?
    } else {
      builder.allow_unauthenticated().build()?
    };
    ServerConfig::builder()
      .with_client_cert_verifier(verifier)
      .with_single_cert(cert_chain, key)?
  } else {
    ServerConfig::builder()
      .with_no_client_auth()
      .with_single_cert(cert_chain, key)?
  };

  Ok(TlsAcceptor::from(Arc::new(server_cfg)))
}
