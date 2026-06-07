#![expect(clippy::unwrap_used, reason = "Fine in tests")]

use axum::http::{HeaderMap, HeaderValue};

use super::*;

fn signed_header_value(secret: &str, body: &[u8]) -> HeaderValue {
  use hmac::{Hmac, Mac};
  use sha2::Sha256;

  let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
  mac.update(body);
  let signature =
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

  HeaderValue::from_str(&signature).unwrap()
}

#[test]
fn test_verify_signature_valid() {
  use hmac::{Hmac, Mac};
  use sha2::Sha256;

  let secret = "test-secret";
  let body = b"test-body";

  let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
  mac.update(body);
  let expected = hex::encode(mac.finalize().into_bytes());

  assert!(verify_signature(
    secret,
    body,
    &format!("sha256={expected}")
  ));
}

#[test]
fn test_verify_signature_invalid() {
  let secret = "test-secret";
  let body = b"test-body";
  assert!(!verify_signature(secret, body, "sha256=invalidsignature"));
}

#[test]
fn test_verify_signature_wrong_secret() {
  use hmac::{Hmac, Mac};
  use sha2::Sha256;

  let body = b"test-body";
  let mut mac = Hmac::<Sha256>::new_from_slice(b"secret1").unwrap();
  mac.update(body);
  let sig = hex::encode(mac.finalize().into_bytes());

  assert!(!verify_signature("secret2", body, &format!("sha256={sig}")));
}

#[test]
fn strip_branch_prefix_handles_heads_and_plain_names() {
  assert_eq!(strip_branch_prefix("refs/heads/main"), "main");
  assert_eq!(strip_branch_prefix("feature"), "feature");
}

#[test]
fn gitea_policy_rejects_valid_forgejo_signature_header() {
  let secret = "test-secret";
  let body = b"test-body";
  let mut headers = HeaderMap::new();

  headers.insert("x-forgejo-signature", signed_header_value(secret, body));

  assert!(!gitea::PROVIDER.is_signature_valid(&headers, body, secret));

  headers.insert("x-gitea-signature", signed_header_value(secret, body));

  assert!(gitea::PROVIDER.is_signature_valid(&headers, body, secret));
}

#[test]
fn forgejo_policy_rejects_valid_gitea_signature_header() {
  let secret = "test-secret";
  let body = b"test-body";
  let mut headers = HeaderMap::new();

  headers.insert("x-gitea-signature", signed_header_value(secret, body));

  assert!(!forgejo::PROVIDER.is_signature_valid(&headers, body, secret));

  headers.insert("x-forgejo-signature", signed_header_value(secret, body));

  assert!(forgejo::PROVIDER.is_signature_valid(&headers, body, secret));
}
