//! Verification of asymmetrically signed JWTs against JWKS keys.
//!
//! Only what OIDC ID tokens need: RSA, ECDSA and Ed25519 signatures through
//! ring, and the registered `exp`, `nbf`, `iss`, `aud` claims. HMAC is never
//! accepted, so a token cannot pick a shared-secret algorithm.

use data_encoding::BASE64URL_NOPAD;
use jiff::Timestamp;
use ring::signature::{
  ECDSA_P256_SHA256_FIXED,
  ECDSA_P384_SHA384_FIXED,
  ED25519,
  RSA_PKCS1_2048_8192_SHA256,
  RSA_PKCS1_2048_8192_SHA384,
  RSA_PKCS1_2048_8192_SHA512,
  RSA_PSS_2048_8192_SHA256,
  RSA_PSS_2048_8192_SHA384,
  RSA_PSS_2048_8192_SHA512,
  RsaPublicKeyComponents,
  UnparsedPublicKey,
  VerificationAlgorithm,
};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;

/// Clock skew tolerated on `exp` and `nbf`.
const LEEWAY_SECS: i64 = 60;

#[derive(Debug, thiserror::Error)]
pub enum JwtError {
  #[error("malformed JWT: {0}")]
  Malformed(&'static str),
  #[error("JWT segment is not valid JSON")]
  Json(#[source] serde_json::Error),
  #[error("JWT algorithm {0:?} is not accepted")]
  Algorithm(Algorithm),
  #[error("JWK does not fit the {0:?} algorithm")]
  Key(Algorithm),
  #[error("JWT signature does not verify")]
  Signature,
  #[error("JWT claim {0} is missing")]
  MissingClaim(&'static str),
  #[error("JWT claim {0} does not validate")]
  InvalidClaim(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Algorithm {
  RS256,
  RS384,
  RS512,
  PS256,
  PS384,
  PS512,
  ES256,
  ES384,
  EdDSA,
  #[serde(other)]
  Other,
}

#[derive(Debug, Deserialize)]
pub struct Header {
  pub alg: Algorithm,
  pub kid: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Jwk {
  pub kty:     String,
  pub kid:     Option<String>,
  #[serde(rename = "use")]
  pub key_use: Option<String>,
  pub alg:     Option<Algorithm>,
  pub n:       Option<String>,
  pub e:       Option<String>,
  pub crv:     Option<String>,
  pub x:       Option<String>,
  pub y:       Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct JwkSet {
  pub keys: Vec<Jwk>,
}

/// Registered claims every token must carry and match.
pub struct Validation<'cfg> {
  pub issuer:    &'cfg str,
  pub audiences: &'cfg [String],
  pub required:  &'cfg [&'static str],
}

fn decode_segment(segment: &str) -> Result<Vec<u8>, JwtError> {
  BASE64URL_NOPAD
    .decode(segment.as_bytes())
    .map_err(|_| JwtError::Malformed("segment is not base64url"))
}

fn split(token: &str) -> Result<(&str, &str, &str), JwtError> {
  let mut parts = token.split('.');
  match (parts.next(), parts.next(), parts.next(), parts.next()) {
    (Some(header), Some(payload), Some(signature), None) => {
      Ok((header, payload, signature))
    },
    _ => Err(JwtError::Malformed("expected three segments")),
  }
}

/// Read the header without verifying anything, to pick the key.
///
/// # Errors
///
/// Returns an error when the token or its header is malformed.
pub fn decode_header(token: &str) -> Result<Header, JwtError> {
  let (header, ..) = split(token)?;
  serde_json::from_slice(&decode_segment(header)?).map_err(JwtError::Json)
}

fn optional_bytes(field: Option<&String>) -> Result<Vec<u8>, JwtError> {
  field.map_or(Err(JwtError::Malformed("JWK lacks a key field")), |value| {
    decode_segment(value)
  })
}

fn verify_signature(
  algorithm: Algorithm,
  key: &Jwk,
  message: &[u8],
  signature: &[u8],
) -> Result<(), JwtError> {
  if key.alg.is_some_and(|alg| alg != algorithm) {
    return Err(JwtError::Key(algorithm));
  }

  let rsa = match algorithm {
    Algorithm::RS256 => Some(&RSA_PKCS1_2048_8192_SHA256),
    Algorithm::RS384 => Some(&RSA_PKCS1_2048_8192_SHA384),
    Algorithm::RS512 => Some(&RSA_PKCS1_2048_8192_SHA512),
    Algorithm::PS256 => Some(&RSA_PSS_2048_8192_SHA256),
    Algorithm::PS384 => Some(&RSA_PSS_2048_8192_SHA384),
    Algorithm::PS512 => Some(&RSA_PSS_2048_8192_SHA512),
    Algorithm::ES256 | Algorithm::ES384 | Algorithm::EdDSA => None,
    Algorithm::Other => return Err(JwtError::Algorithm(algorithm)),
  };

  let verified = if let Some(parameters) = rsa {
    if key.kty != "RSA" {
      return Err(JwtError::Key(algorithm));
    }
    RsaPublicKeyComponents {
      n: optional_bytes(key.n.as_ref())?,
      e: optional_bytes(key.e.as_ref())?,
    }
    .verify(parameters, message, signature)
  } else {
    let (kty, crv, verification): (_, _, &'static dyn VerificationAlgorithm) =
      match algorithm {
        Algorithm::ES256 => ("EC", "P-256", &ECDSA_P256_SHA256_FIXED),
        Algorithm::ES384 => ("EC", "P-384", &ECDSA_P384_SHA384_FIXED),
        _ => ("OKP", "Ed25519", &ED25519),
      };
    if key.kty != kty || key.crv.as_deref() != Some(crv) {
      return Err(JwtError::Key(algorithm));
    }

    let mut public = optional_bytes(key.x.as_ref())?;
    if kty == "EC" {
      public.insert(0, 0x04);
      public.extend(optional_bytes(key.y.as_ref())?);
    }
    UnparsedPublicKey::new(verification, public).verify(message, signature)
  };

  verified.map_err(|_| JwtError::Signature)
}

fn validate_claims(
  claims: &Value,
  validation: &Validation<'_>,
) -> Result<(), JwtError> {
  for claim in validation.required {
    if claims.get(claim).is_none() {
      return Err(JwtError::MissingClaim(claim));
    }
  }

  let now = Timestamp::now().as_second();
  let seconds = |name| {
    claims.get(name).map(|value: &Value| {
      value
        .as_i64()
        .or_else(|| value.as_f64().map(|secs| secs.floor() as i64))
    })
  };

  match seconds("exp") {
    Some(Some(exp)) if exp.saturating_add(LEEWAY_SECS) < now => {
      return Err(JwtError::InvalidClaim("exp"));
    },
    Some(None) => return Err(JwtError::InvalidClaim("exp")),
    _ => {},
  }
  match seconds("nbf") {
    Some(Some(nbf)) if nbf.saturating_sub(LEEWAY_SECS) > now => {
      return Err(JwtError::InvalidClaim("nbf"));
    },
    Some(None) => return Err(JwtError::InvalidClaim("nbf")),
    _ => {},
  }

  if claims.get("iss").and_then(Value::as_str) != Some(validation.issuer) {
    return Err(JwtError::InvalidClaim("iss"));
  }

  let audience_matches = |aud: &Value| {
    aud
      .as_str()
      .is_some_and(|aud| validation.audiences.iter().any(|want| want == aud))
  };
  let audience = match claims.get("aud") {
    Some(Value::Array(audiences)) => audiences.iter().any(audience_matches),
    Some(aud) => audience_matches(aud),
    None => false,
  };
  if !audience {
    return Err(JwtError::InvalidClaim("aud"));
  }

  Ok(())
}

/// Verify `token`'s signature with `key` and its registered claims, then
/// deserialize the payload.
///
/// # Errors
///
/// Returns an error for a malformed token, an algorithm or key that does not
/// fit, a bad signature, or a missing or failing registered claim.
pub fn decode<T: DeserializeOwned>(
  token: &str,
  key: &Jwk,
  validation: &Validation<'_>,
) -> Result<T, JwtError> {
  let header = decode_header(token)?;
  let (encoded_header, payload, signature) = split(token)?;
  let message = &token[..encoded_header.len() + 1 + payload.len()];
  verify_signature(
    header.alg,
    key,
    message.as_bytes(),
    &decode_segment(signature)?,
  )?;

  let claims: Value = serde_json::from_slice(&decode_segment(payload)?)
    .map_err(JwtError::Json)?;
  validate_claims(&claims, validation)?;
  serde_json::from_value(claims).map_err(JwtError::Json)
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "fine in tests")]
mod tests {
  use ring::{
    rand::SystemRandom,
    signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair},
  };

  use super::*;

  #[test]
  fn verifies_signatures_and_rejects_hmac() {
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
    let key = Jwk {
      kty:     "EC".to_owned(),
      kid:     None,
      key_use: None,
      alg:     None,
      n:       None,
      e:       None,
      crv:     Some("P-256".to_owned()),
      x:       Some(BASE64URL_NOPAD.encode(x)),
      y:       Some(BASE64URL_NOPAD.encode(y)),
    };
    let audiences = ["circus".to_owned()];
    let validation = Validation {
      issuer:    "https://issuer",
      audiences: &audiences,
      required:  &["exp"],
    };
    let exp = Timestamp::now().as_second() + 600;
    let sign = |header: &str| {
      let payload =
        format!(r#"{{"iss":"https://issuer","aud":"circus","exp":{exp}}}"#);
      let message = format!(
        "{}.{}",
        BASE64URL_NOPAD.encode(header.as_bytes()),
        BASE64URL_NOPAD.encode(payload.as_bytes())
      );
      let signature = pair.sign(&rng, message.as_bytes()).unwrap();
      (message, signature.as_ref().to_vec())
    };
    let verify = |message: &str, signature: &[u8]| {
      let token = format!("{message}.{}", BASE64URL_NOPAD.encode(signature));
      decode::<Value>(&token, &key, &validation)
    };

    let (message, mut signature) = sign(r#"{"alg":"ES256"}"#);
    assert!(verify(&message, &signature).is_ok());
    signature[0] ^= 1;
    assert!(matches!(
      verify(&message, &signature),
      Err(JwtError::Signature)
    ));

    let (message, signature) = sign(r#"{"alg":"HS256"}"#);
    assert!(matches!(
      verify(&message, &signature),
      Err(JwtError::Algorithm(Algorithm::Other))
    ));
  }
}
