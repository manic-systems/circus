//! Shared Nix narinfo fingerprint signing.

use std::path::Path;

use color_eyre::eyre::{Context as _, Result, eyre};
use harmonia_utils_signature::SecretKey;

/// Read a Nix-format `<name>:<base64 secret>` signing key from disk.
///
/// # Errors
///
/// Returns an error when the file cannot be read, or the key is malformed or
/// its public half does not match its seed.
pub async fn read_signing_key(path: &Path) -> Result<SecretKey> {
  let raw = tokio::fs::read_to_string(path)
    .await
    .with_context(|| format!("read signing key {}", path.display()))?;
  raw
    .trim()
    .parse::<SecretKey>()
    .map_err(|e| eyre!("invalid signing key: {e:?}"))
}

/// Sign the canonical Nix narinfo fingerprint, returning
/// `<name>:<base64 signature>`. References are signed as a sorted set to match
/// Nix's verification.
#[must_use]
pub fn sign_narinfo(
  key: &SecretKey,
  store_path: &str,
  nar_hash: &str,
  nar_size: i64,
  references: &[String],
) -> String {
  let mut sorted_refs = references.to_vec();
  sorted_refs.sort();
  let fingerprint = format!(
    "1;{store_path};{nar_hash};{nar_size};{}",
    sorted_refs.join(",")
  );
  key.sign(fingerprint.as_bytes()).to_string()
}
