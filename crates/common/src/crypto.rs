//! Process-wide rustls crypto provider setup and small crypto helpers.

use base64::{Engine as _, engine::general_purpose::STANDARD};
use ring::{aead, hkdf, rand};

use crate::error::{CiError, Result};

const WEBHOOK_SECRET_PREFIX: &str = "v1";
const NONCE_LEN: usize = 12;

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

/// Encrypt a secret for database storage.
///
/// Used for webhook secrets and per-project notification secrets (forge tokens,
/// Slack URLs, SMTP passwords). The output is a self-describing
/// `v1:<nonce>:<ciphertext>` string; the same AEAD key derivation is shared
/// across all secret kinds.
///
/// # Errors
///
/// Returns an error when no key is configured or encryption fails.
pub fn encrypt_secret(secret: &str, key: Option<&str>) -> Result<String> {
  let key = secret_aead_key(key)?;
  let rng = rand::SystemRandom::new();
  let mut nonce_bytes = [0u8; NONCE_LEN];
  rand::SecureRandom::fill(&rng, &mut nonce_bytes)
    .map_err(|_| CiError::Config("Failed to generate secret nonce".into()))?;

  let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);
  let mut ciphertext = secret.as_bytes().to_vec();
  key
    .seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut ciphertext)
    .map_err(|_| CiError::Config("Failed to encrypt secret".into()))?;

  Ok(format!(
    "{WEBHOOK_SECRET_PREFIX}:{}:{}",
    STANDARD.encode(nonce_bytes),
    STANDARD.encode(ciphertext)
  ))
}

/// Decrypt a secret loaded from database storage.
///
/// Plaintext values (those without the `v1:` prefix) are returned unchanged so
/// existing configured secrets keep working until they are recreated or
/// upserted.
///
/// # Errors
///
/// Returns an error when encrypted data cannot be decrypted.
pub fn decrypt_secret(value: &str, key: Option<&str>) -> Result<String> {
  let Some(rest) = value.strip_prefix("v1:") else {
    return Ok(value.to_string());
  };
  let (nonce, ciphertext) = rest
    .split_once(':')
    .ok_or_else(|| CiError::Config("Invalid encrypted secret format".into()))?;

  let key = secret_aead_key(key)?;
  let nonce_bytes = STANDARD
    .decode(nonce)
    .map_err(|_| CiError::Config("Invalid secret nonce".into()))?;
  let nonce = aead::Nonce::try_assume_unique_for_key(&nonce_bytes)
    .map_err(|_| CiError::Config("Invalid secret nonce".into()))?;
  let mut plaintext = STANDARD
    .decode(ciphertext)
    .map_err(|_| CiError::Config("Invalid secret ciphertext".into()))?;

  let plaintext = key
    .open_in_place(nonce, aead::Aad::empty(), &mut plaintext)
    .map_err(|_| CiError::Config("Failed to decrypt secret".into()))?;
  String::from_utf8(plaintext.to_vec())
    .map_err(|_| CiError::Config("Secret is not valid UTF-8".into()))
}

/// Encrypt a webhook secret for database storage.
///
/// Thin wrapper over [`encrypt_secret`] retained for call-site clarity.
///
/// # Errors
///
/// Returns an error when no key is configured or encryption fails.
pub fn encrypt_webhook_secret(
  secret: &str,
  key: Option<&str>,
) -> Result<String> {
  encrypt_secret(secret, key)
}

/// Decrypt a webhook secret loaded from database storage.
///
/// Thin wrapper over [`decrypt_secret`] retained for call-site clarity.
///
/// # Errors
///
/// Returns an error when encrypted data cannot be decrypted.
pub fn decrypt_webhook_secret(
  value: &str,
  key: Option<&str>,
) -> Result<String> {
  decrypt_secret(value, key)
}

struct Aes256KeyLen;

impl hkdf::KeyType for Aes256KeyLen {
  fn len(&self) -> usize {
    32
  }
}

fn secret_aead_key(key: Option<&str>) -> Result<aead::LessSafeKey> {
  let key = key.filter(|key| !key.trim().is_empty()).ok_or_else(|| {
    CiError::Config("server.webhook_secret_encryption_key is required".into())
  })?;

  let salt = hkdf::Salt::new(hkdf::HKDF_SHA256, b"circus-webhook-secret-v1");
  let prk = salt.extract(key.as_bytes());
  let okm = prk
    .expand(&[b"aes-256-gcm-key"], Aes256KeyLen)
    .map_err(|_| {
      CiError::Config("HKDF expand failed for webhook encryption key".into())
    })?;
  let mut key_bytes = [0u8; 32];
  okm.fill(&mut key_bytes).map_err(|_| {
    CiError::Config("HKDF fill failed for webhook encryption key".into())
  })?;
  let unbound =
    aead::UnboundKey::new(&aead::AES_256_GCM, &key_bytes).map_err(|_| {
      CiError::Config("Invalid webhook secret encryption key".into())
    })?;
  Ok(aead::LessSafeKey::new(unbound))
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "fine in tests")]
mod tests {
  use super::{decrypt_webhook_secret, encrypt_webhook_secret};

  #[test]
  fn encrypt_webhook_secret_requires_key() {
    let err = encrypt_webhook_secret("secret", None).unwrap_err();

    assert_eq!(
      err.to_string(),
      "Configuration error: server.webhook_secret_encryption_key is required"
    );
  }

  #[test]
  fn encrypt_webhook_secret_rejects_blank_key() {
    let err = encrypt_webhook_secret("secret", Some("  ")).unwrap_err();

    assert_eq!(
      err.to_string(),
      "Configuration error: server.webhook_secret_encryption_key is required"
    );
  }

  #[test]
  fn webhook_secret_round_trips_with_key() {
    let encrypted = encrypt_webhook_secret("secret", Some("test-key")).unwrap();

    assert_ne!(encrypted, "secret");
    assert!(encrypted.starts_with("v1:"));
    assert_eq!(
      decrypt_webhook_secret(&encrypted, Some("test-key")).unwrap(),
      "secret"
    );
  }
}
