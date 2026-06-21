//! Derivation of the public binary-cache signing key from the configured
//! secret key.
//!
//! Nix Ed25519 cache keys are stored as `<name>:<base64(64 bytes)>`, where the
//! 64 bytes are the 32-byte seed followed by the 32-byte public key. The server
//! only holds the secret (`signing.key_file`); consumers need the matching
//! public key for `trusted-public-keys`. We recover it by slicing the already
//! base64-decoded bytes, so no signing crate is required.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use circus_config::Config;

/// Length of a full Nix secret key payload: 32-byte seed + 32-byte public key.
const SECRET_KEY_BYTES: usize = 64;

/// Derive the public signing key (`<name>:<base64(public)>`) from the secret
/// key referenced by `signing.key_file`.
///
/// # Returns
///
/// `None` when signing is disabled, no key file is configured, the file cannot
/// be read, or its contents are not a well-formed `<name>:<base64>` secret key.
#[must_use]
pub fn signing_public_key(config: &Config) -> Option<String> {
  if !config.signing.enabled {
    return None;
  }
  let path = config.signing.key_file.as_ref()?;
  let contents = std::fs::read_to_string(path).ok()?;
  public_key_from_secret(contents.trim())
}

/// Pure transform of a `<name>:<base64(64 bytes)>` secret key into its
/// `<name>:<base64(32-byte public)>` counterpart.
fn public_key_from_secret(secret: &str) -> Option<String> {
  let (name, b64) = secret.split_once(':')?;
  if name.is_empty() {
    return None;
  }
  let bytes = STANDARD.decode(b64.trim()).ok()?;
  if bytes.len() != SECRET_KEY_BYTES {
    return None;
  }
  let public = STANDARD.encode(&bytes[32..SECRET_KEY_BYTES]);
  Some(format!("{name}:{public}"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn derives_public_from_secret() {
    // 64 bytes: first 32 are the seed (0x00..), last 32 are the "public" half
    // (0x01..). The derived key must echo the second half only.
    let mut raw = vec![0u8; 32];
    raw.extend(std::iter::repeat_n(1u8, 32));
    let secret = format!("ci.example.org-1:{}", STANDARD.encode(&raw));

    let public = public_key_from_secret(&secret).expect("valid secret key");
    let expected_b64 = STANDARD.encode([1u8; 32]);
    assert_eq!(public, format!("ci.example.org-1:{expected_b64}"));
  }

  #[test]
  fn rejects_malformed() {
    assert!(public_key_from_secret("no-colon").is_none());
    assert!(public_key_from_secret(":onlybase64").is_none());
    assert!(public_key_from_secret("name:not-base64!!").is_none());
    // Wrong length (32 bytes, not 64).
    let short = format!("name:{}", STANDARD.encode([0u8; 32]));
    assert!(public_key_from_secret(&short).is_none());
  }
}
