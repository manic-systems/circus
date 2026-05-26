use std::time::{SystemTime, UNIX_EPOCH};

use circus_common::notifications::*;

#[test]
fn test_rate_limit_extraction() {
  let mut headers = reqwest::header::HeaderMap::new();
  headers.insert("X-RateLimit-Limit", "5000".parse().unwrap());
  headers.insert("X-RateLimit-Remaining", "1234".parse().unwrap());
  headers.insert("X-RateLimit-Reset", "1735689600".parse().unwrap());

  let state = extract_rate_limit_from_headers(&headers);
  assert!(state.is_some());

  let state = state.unwrap();
  assert_eq!(state.limit, 5000);
  assert_eq!(state.remaining, 1234);
  assert_eq!(state.reset_at, 1735689600);
}

#[test]
fn test_rate_limit_missing_headers() {
  let headers = reqwest::header::HeaderMap::new();
  let state = extract_rate_limit_from_headers(&headers);
  assert!(state.is_none());
}

#[test]
fn test_sleep_duration_calculation() {
  let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_secs();

  let state = RateLimitState {
    limit:     5000,
    remaining: 500,
    reset_at:  now + 3600,
  };

  let delay = calculate_delay(&state, now);
  assert!((6..=7).contains(&delay));
}

#[test]
fn test_sleep_duration_minimum() {
  let now = SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .unwrap()
    .as_secs();

  let state = RateLimitState {
    limit:     5000,
    remaining: 4999,
    reset_at:  now + 10000,
  };

  let delay = calculate_delay(&state, now);
  assert_eq!(delay, 1);
}
