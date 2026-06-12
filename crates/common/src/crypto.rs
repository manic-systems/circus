//! Process-wide rustls crypto provider setup.

/// Pin ring as the process-level rustls [`CryptoProvider`].
///
/// # Errors
///
/// Returns an error if a provider was already installed, which should never
/// happen.
pub fn install_crypto_provider() -> color_eyre::Result<()> {
  rustls::crypto::ring::default_provider()
    .install_default()
    .map_err(|_| {
      color_eyre::eyre::eyre!("a rustls CryptoProvider is already installed")
    })
}
