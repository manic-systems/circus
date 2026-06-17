use std::{
  sync::{Arc, Mutex},
  time::{Duration, Instant},
};

use axum::{
  Extension,
  Json,
  body::Body,
  extract::{Path, Request},
  http::StatusCode,
  middleware::Next,
  response::{IntoResponse, Response},
};
use uuid::Uuid;

use super::WebhookResponse;

struct WebhookBucket {
  tokens:        f64,
  last_refilled: Instant,
}

impl WebhookBucket {
  const fn new(tokens: f64, now: Instant) -> Self {
    Self {
      tokens,
      last_refilled: now,
    }
  }

  fn is_expired(&self, now: Instant) -> bool {
    now.duration_since(self.last_refilled) >= WEBHOOK_BUCKET_TTL
  }

  fn refill(&mut self, rps: f64, burst: f64, now: Instant) {
    let elapsed = now.duration_since(self.last_refilled).as_secs_f64();
    self.tokens = elapsed.mul_add(rps, self.tokens).min(burst);
    self.last_refilled = now;
  }

  fn try_consume(&mut self) -> bool {
    if self.tokens < 1.0 {
      return false;
    }
    self.tokens -= 1.0;
    true
  }
}

pub(super) struct WebhookRateLimiter {
  buckets:      dashmap::DashMap<Uuid, WebhookBucket>,
  rps:          f64,
  burst:        f64,
  last_cleanup: Mutex<Instant>,
}

pub(super) const WEBHOOK_PROJECT_RATE_LIMIT: u32 = 10;
pub(super) const WEBHOOK_RATE_LIMIT_WINDOW: Duration = Duration::from_mins(1);
const WEBHOOK_BUCKET_TTL: Duration = Duration::from_mins(5);
const WEBHOOK_BUCKET_CLEANUP_INTERVAL: Duration = Duration::from_secs(30);

impl WebhookRateLimiter {
  pub(super) fn new() -> Self {
    Self::new_at(Instant::now())
  }

  pub(super) fn new_at(now: Instant) -> Self {
    Self {
      buckets:      dashmap::DashMap::new(),
      rps:          f64::from(WEBHOOK_PROJECT_RATE_LIMIT)
        / WEBHOOK_RATE_LIMIT_WINDOW.as_secs_f64(),
      burst:        f64::from(WEBHOOK_PROJECT_RATE_LIMIT),
      last_cleanup: Mutex::new(now),
    }
  }

  pub(super) fn allow(&self, project_id: Uuid, now: Instant) -> bool {
    self.cleanup_expired(now);

    let mut bucket = self
      .buckets
      .entry(project_id)
      .or_insert_with(|| WebhookBucket::new(self.burst, now));

    bucket.refill(self.rps, self.burst, now);
    bucket.try_consume()
  }

  fn cleanup_expired(&self, now: Instant) {
    let Ok(mut last_cleanup) = self.last_cleanup.try_lock() else {
      return;
    };
    if now.duration_since(*last_cleanup) < WEBHOOK_BUCKET_CLEANUP_INTERVAL {
      return;
    }

    self.buckets.retain(|_, bucket| !bucket.is_expired(now));
    *last_cleanup = now;
  }
}

pub(super) async fn middleware(
  Path(project_id): Path<Uuid>,
  Extension(limiter): Extension<Arc<WebhookRateLimiter>>,
  request: Request<Body>,
  next: Next,
) -> Response {
  if !limiter.allow(project_id, Instant::now()) {
    return (
      StatusCode::TOO_MANY_REQUESTS,
      Json(WebhookResponse {
        accepted: false,
        message:  "Webhook rate limit exceeded".to_string(),
      }),
    )
      .into_response();
  }

  next.run(request).await
}
