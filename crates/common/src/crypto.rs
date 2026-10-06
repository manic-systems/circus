//! Process-wide rustls and jsonwebtoken crypto provider setup and small
//! crypto helpers.

use data_encoding::BASE64;
use jsonwebtoken::{
  Algorithm,
  DecodingKey,
  DecodingKeyKind,
  crypto::{CryptoProvider, JwtVerifier, KeyUtils},
  errors::ErrorKind,
  signature::{self, Verifier},
};
use ring::{
  aead,
  hkdf,
  rand,
  signature::{
    ECDSA_P256_SHA256_FIXED,
    ECDSA_P384_SHA384_FIXED,
    ED25519,
    RSA_PKCS1_2048_8192_SHA256,
    RSA_PKCS1_2048_8192_SHA384,
    RSA_PKCS1_2048_8192_SHA512,
    RSA_PSS_2048_8192_SHA256,
    RSA_PSS_2048_8192_SHA384,
    RSA_PSS_2048_8192_SHA512,
    RsaParameters,
    RsaPublicKeyComponents,
    UnparsedPublicKey,
    VerificationAlgorithm,
  },
};

use crate::error::{CiError, Result};

const WEBHOOK_SECRET_PREFIX: &str = "v1";
const NONCE_LEN: usize = 12;

/// Pin ring as the process-level rustls and jsonwebtoken crypto provider.
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
    })?;
  JWT_PROVIDER.install_default().map_err(|_| {
    color_eyre::eyre::eyre!(
      "a jsonwebtoken CryptoProvider is already installed"
    )
  })
}

/// Verify-only, so signing and HMAC algorithms are rejected.
static JWT_PROVIDER: CryptoProvider = CryptoProvider {
  signer_factory:   |_, _| Err(ErrorKind::InvalidAlgorithm.into()),
  verifier_factory: jwt_verifier,
  key_utils:        KeyUtils::new_unimplemented(),
};

struct RingVerifier {
  algorithm:    Algorithm,
  verification: &'static dyn VerificationAlgorithm,
  key:          DecodingKey,
}

const fn rsa_parameters(
  algorithm: Algorithm,
) -> Option<&'static RsaParameters> {
  match algorithm {
    Algorithm::RS256 => Some(&RSA_PKCS1_2048_8192_SHA256),
    Algorithm::RS384 => Some(&RSA_PKCS1_2048_8192_SHA384),
    Algorithm::RS512 => Some(&RSA_PKCS1_2048_8192_SHA512),
    Algorithm::PS256 => Some(&RSA_PSS_2048_8192_SHA256),
    Algorithm::PS384 => Some(&RSA_PSS_2048_8192_SHA384),
    Algorithm::PS512 => Some(&RSA_PSS_2048_8192_SHA512),
    _ => None,
  }
}

#[expect(
  clippy::trivially_copy_pass_by_ref,
  reason = "signature is fixed by CryptoProvider::verifier_factory"
)]
fn jwt_verifier(
  algorithm: &Algorithm,
  key: &DecodingKey,
) -> jsonwebtoken::errors::Result<Box<dyn JwtVerifier>> {
  let verification: &'static dyn VerificationAlgorithm = match algorithm {
    Algorithm::ES256 => &ECDSA_P256_SHA256_FIXED,
    Algorithm::ES384 => &ECDSA_P384_SHA384_FIXED,
    Algorithm::EdDSA => &ED25519,
    _ => rsa_parameters(*algorithm).ok_or(ErrorKind::InvalidAlgorithm)?,
  };

  if key.family() != algorithm.family() {
    return Err(ErrorKind::InvalidKeyFormat.into());
  }

  Ok(Box::new(RingVerifier {
    algorithm: *algorithm,
    verification,
    key: key.clone(),
  }))
}

impl Verifier<Vec<u8>> for RingVerifier {
  fn verify(
    &self,
    message: &[u8],
    signature: &Vec<u8>,
  ) -> std::result::Result<(), signature::Error> {
    let verified = match self.key.kind() {
      DecodingKeyKind::RsaModulusExponent { n, e } => {
        let parameters =
          rsa_parameters(self.algorithm).ok_or_else(signature::Error::new)?;
        RsaPublicKeyComponents { n, e }.verify(parameters, message, signature)
      },
      DecodingKeyKind::SecretOrDer(bytes) => {
        UnparsedPublicKey::new(self.verification, bytes)
          .verify(message, signature)
      },
    };

    verified.map_err(|_| signature::Error::new())
  }
}

impl JwtVerifier for RingVerifier {
  fn algorithm(&self) -> Algorithm {
    self.algorithm
  }
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
    BASE64.encode(&nonce_bytes),
    BASE64.encode(&ciphertext)
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
  let nonce_bytes = BASE64
    .decode(nonce.as_bytes())
    .map_err(|_| CiError::Config("Invalid secret nonce".into()))?;
  let nonce = aead::Nonce::try_assume_unique_for_key(&nonce_bytes)
    .map_err(|_| CiError::Config("Invalid secret nonce".into()))?;
  let mut plaintext = BASE64
    .decode(ciphertext.as_bytes())
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
  use data_encoding::BASE64URL_NOPAD;
  use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
  use ring::{
    rand::SystemRandom,
    signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair},
  };

  use super::{JWT_PROVIDER, decrypt_webhook_secret, encrypt_webhook_secret};

  fn verify(token: &str, key: &DecodingKey, algorithm: Algorithm) -> bool {
    let _ = JWT_PROVIDER.install_default();
    let mut validation = Validation::new(algorithm);
    validation.required_spec_claims.clear();
    validation.validate_exp = false;
    decode::<serde_json::Value>(token, key, &validation).is_ok()
  }

  #[test]
  fn jwt_provider_verifies_signatures_and_rejects_hmac() {
    let rng = SystemRandom::new();
    let pkcs8 =
      EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng)
        .unwrap();
    let pair = EcdsaKeyPair::from_pkcs8(
      &ECDSA_P256_SHA256_FIXED_SIGNING,
      pkcs8.as_ref(),
      &rng,
    )
    .unwrap();
    let (x, y) = pair.public_key().as_ref()[1..].split_at(32);
    let key = DecodingKey::from_ec_components(
      &BASE64URL_NOPAD.encode(x),
      &BASE64URL_NOPAD.encode(y),
    )
    .unwrap();
    let sign = |header: &str| {
      let message = format!(
        "{}.{}",
        BASE64URL_NOPAD.encode(header.as_bytes()),
        BASE64URL_NOPAD.encode(br#"{"sub":"agent"}"#)
      );
      let signature = pair.sign(&rng, message.as_bytes()).unwrap();
      (message, signature.as_ref().to_vec())
    };
    let token = |message: &str, signature: &[u8]| {
      format!("{message}.{}", BASE64URL_NOPAD.encode(signature))
    };

    let (message, mut signature) = sign(r#"{"alg":"ES256"}"#);
    assert!(verify(&token(&message, &signature), &key, Algorithm::ES256));
    signature[0] ^= 1;
    assert!(!verify(
      &token(&message, &signature),
      &key,
      Algorithm::ES256
    ));

    let (message, signature) = sign(r#"{"alg":"HS256"}"#);
    let hmac = DecodingKey::from_secret(b"shared");
    assert!(!verify(
      &token(&message, &signature),
      &hmac,
      Algorithm::HS256
    ));
  }

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
