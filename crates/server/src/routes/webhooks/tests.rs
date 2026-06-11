#![expect(clippy::unwrap_used, reason = "Fine in tests")]

use axum::http::{HeaderMap, HeaderValue};

use super::{
  rate_limit::{
    WEBHOOK_PROJECT_RATE_LIMIT,
    WEBHOOK_RATE_LIMIT_WINDOW,
    WebhookRateLimiter,
  },
  *,
};

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

fn test_jobset(
  branch: Option<&str>,
  branch_pattern: Option<&str>,
  tag_pattern: Option<&str>,
) -> Jobset {
  Jobset {
    id:                uuid::Uuid::new_v4(),
    project_id:        uuid::Uuid::new_v4(),
    name:              "checks".to_string(),
    nix_expression:    ".".to_string(),
    enabled:           true,
    flake_mode:        true,
    check_interval:    60,
    trigger_mode:      JobsetTriggerMode::SourceChange,
    branch:            branch.map(str::to_string),
    branch_pattern:    branch_pattern.map(str::to_string),
    tag_pattern:       tag_pattern.map(str::to_string),
    scheduling_shares: 100,
    created_at:        chrono::Utc::now(),
    updated_at:        chrono::Utc::now(),
    state:             circus_common::models::JobsetState::Enabled,
    last_checked_at:   None,
    keep_nr:           3,
  }
}

#[test]
fn parse_push_ref_handles_branches_tags_and_unknown_refs() {
  assert_eq!(parse_push_ref("refs/heads/main"), PushedRef::Branch("main"));
  assert_eq!(parse_push_ref("refs/tags/v1.0"), PushedRef::Tag("v1.0"));
  assert_eq!(parse_push_ref("feature"), PushedRef::Other("feature"));
}

#[test]
fn jobset_push_ref_matching_supports_branch_and_tag_patterns() {
  let legacy_any = test_jobset(None, None, None);
  assert!(jobset_matches_push_ref(
    &legacy_any,
    PushedRef::Branch("feature")
  ));
  assert!(!jobset_matches_push_ref(
    &legacy_any,
    PushedRef::Tag("v1.0")
  ));

  let legacy_branch = test_jobset(Some("main"), None, None);
  assert!(jobset_matches_push_ref(
    &legacy_branch,
    PushedRef::Branch("main")
  ));
  assert!(!jobset_matches_push_ref(
    &legacy_branch,
    PushedRef::Branch("release")
  ));

  let release_branches = test_jobset(None, Some("release-*"), None);
  assert!(jobset_matches_push_ref(
    &release_branches,
    PushedRef::Branch("release-2026")
  ));
  assert!(!jobset_matches_push_ref(
    &release_branches,
    PushedRef::Branch("main")
  ));

  let release_tags = test_jobset(None, None, Some("v1.*"));
  assert!(jobset_matches_push_ref(
    &release_tags,
    PushedRef::Tag("v1.2")
  ));
  assert!(!jobset_matches_push_ref(
    &release_tags,
    PushedRef::Branch("main")
  ));

  let branch_and_tags = test_jobset(Some("main"), None, Some("v1.*"));
  assert!(jobset_matches_push_ref(
    &branch_and_tags,
    PushedRef::Branch("main")
  ));
  assert!(jobset_matches_push_ref(
    &branch_and_tags,
    PushedRef::Tag("v1.2")
  ));
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

#[test]
fn webhook_rate_limiter_is_per_project() {
  let limiter = WebhookRateLimiter::new();
  let first = uuid::Uuid::new_v4();
  let second = uuid::Uuid::new_v4();
  let now = std::time::Instant::now();

  for _ in 0..WEBHOOK_PROJECT_RATE_LIMIT {
    assert!(limiter.allow(first, now));
  }
  assert!(!limiter.allow(first, now));
  assert!(limiter.allow(second, now));
}

#[test]
fn webhook_rate_limiter_refills() {
  let limiter = WebhookRateLimiter::new();
  let project_id = uuid::Uuid::new_v4();
  let now = std::time::Instant::now();

  for _ in 0..WEBHOOK_PROJECT_RATE_LIMIT {
    assert!(limiter.allow(project_id, now));
  }
  assert!(!limiter.allow(project_id, now));
  assert!(limiter.allow(project_id, now + WEBHOOK_RATE_LIMIT_WINDOW));
}
