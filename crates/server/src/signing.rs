//! The public binary-cache signing key that consumers add to
//! `trusted-public-keys`.
//!
//! Deployments normally give the server only `signing.public_key`, so it never
//! holds the secret. Without it, the key is recovered from `signing.key_file`:
//! a Nix Ed25519 secret key is `<name>:<base64(64 bytes)>`, a 32-byte seed
//! followed by the 32-byte public key, so slicing the decoded bytes is enough.

use circus_config::{Config, public_key_from_secret};

/// `signing.public_key` when set, otherwise the public half of
/// `signing.key_file`.
///
/// # Returns
///
/// `None` when signing is disabled, neither key is configured, the key file
/// cannot be read, or it is not a well-formed `<name>:<base64>` secret key.
#[must_use]
pub fn signing_public_key(config: &Config) -> Option<String> {
  if !config.signing.enabled {
    return None;
  }
  if let Some(public_key) = &config.signing.public_key {
    return Some(public_key.clone());
  }
  let path = config.signing.key_file.as_ref()?;
  let contents = std::fs::read_to_string(path).ok()?;
  public_key_from_secret(contents.trim())
}
