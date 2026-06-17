use std::{
  sync::Arc,
  time::{Duration, Instant},
};

use axum::{
  Extension,
  Json,
  Router,
  body::Body,
  extract::Request,
  http::{HeaderMap, StatusCode},
  middleware::{self, Next},
  response::{IntoResponse, Response},
  routing::post,
};
use circus_common::{
  models::{CreateEvaluation, Jobset, JobsetTriggerMode},
  repo,
};
use hmac::KeyInit;
use serde::Serialize;
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

mod forgejo;
mod gitea;
mod gitea_compatible;
mod github;
mod gitlab;

#[cfg(test)] mod tests;

#[derive(Debug, Serialize)]
struct WebhookResponse {
  accepted: bool,
  message:  String,
}

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

struct WebhookRateLimiter {
  buckets: dashmap::DashMap<Uuid, WebhookBucket>,
  rps:     f64,
  burst:   f64,
}

const WEBHOOK_PROJECT_RATE_LIMIT: u32 = 10;
const WEBHOOK_RATE_LIMIT_WINDOW: Duration = Duration::from_mins(1);
const WEBHOOK_BUCKET_TTL: Duration = Duration::from_mins(5);

impl WebhookRateLimiter {
  fn new() -> Self {
    Self {
      buckets: dashmap::DashMap::new(),
      rps:     f64::from(WEBHOOK_PROJECT_RATE_LIMIT)
        / WEBHOOK_RATE_LIMIT_WINDOW.as_secs_f64(),
      burst:   f64::from(WEBHOOK_PROJECT_RATE_LIMIT),
    }
  }

  fn allow(&self, project_id: Uuid, now: Instant) -> bool {
    self.buckets.retain(|_, bucket| !bucket.is_expired(now));

    let mut bucket = self
      .buckets
      .entry(project_id)
      .or_insert_with(|| WebhookBucket::new(self.burst, now));

    bucket.refill(self.rps, self.burst, now);
    bucket.try_consume()
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WebhookPath {
  project_id: Uuid,
}

impl WebhookPath {
  fn parse(path: &str) -> Option<Self> {
    let mut segments = path.split('/').filter(|segment| !segment.is_empty());
    if segments.next()? != "api" {
      return None;
    }
    if segments.next()? != "v1" {
      return None;
    }
    if segments.next()? != "webhooks" {
      return None;
    }
    let project_id = segments
      .next()
      .and_then(|segment| Uuid::parse_str(segment).ok())?;
    Some(Self { project_id })
  }

  const fn project_id(self) -> Uuid {
    self.project_id
  }
}

async fn webhook_rate_limit_middleware(
  Extension(limiter): Extension<Arc<WebhookRateLimiter>>,
  request: Request<Body>,
  next: Next,
) -> Response {
  let Some(path) = WebhookPath::parse(request.uri().path()) else {
    return next.run(request).await;
  };

  if !limiter.allow(path.project_id(), Instant::now()) {
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

pub fn router() -> Router<AppState> {
  Router::new()
    .route(
      "/api/v1/webhooks/{project_id}/github",
      post(github::handle_webhook),
    )
    .route(
      "/api/v1/webhooks/{project_id}/gitea",
      post(gitea::handle_webhook),
    )
    .route(
      "/api/v1/webhooks/{project_id}/forgejo",
      post(forgejo::handle_webhook),
    )
    .route(
      "/api/v1/webhooks/{project_id}/gitlab",
      post(gitlab::handle_webhook),
    )
    .layer(middleware::from_fn(webhook_rate_limit_middleware))
    .layer(Extension(Arc::new(WebhookRateLimiter::new())))
}

fn trace_webhook_repo(
  forge: &str,
  project_id: Uuid,
  clone_url: Option<&str>,
  html_url: Option<&str>,
) {
  tracing::debug!(
    forge,
    %project_id,
    clone_url,
    html_url,
    "webhook payload repository"
  );
}

/// Strip the `refs/heads/` prefix from a git ref. Returns the original
/// string if no such prefix is present.
fn strip_branch_prefix(git_ref: &str) -> &str {
  git_ref.strip_prefix("refs/heads/").unwrap_or(git_ref)
}

/// True if a jobset configured for `jobset_branch` should react to a push
/// to `pushed_branch`. A jobset with no configured branch matches every
/// push (treat None as "any branch").
fn jobset_matches_branch(
  jobset_branch: Option<&str>,
  pushed_branch: &str,
) -> bool {
  jobset_branch.is_none_or(|b| b == pushed_branch)
}

fn jobset_accepts_source_trigger(jobset: &Jobset) -> bool {
  jobset.enabled && jobset.trigger_mode == JobsetTriggerMode::SourceChange
}

fn is_deleted_commit(commit: &str) -> bool {
  commit.is_empty() || commit == "0000000000000000000000000000000000000000"
}

async fn trigger_push_evaluations(
  state: &AppState,
  project_id: Uuid,
  commit: &str,
  pushed_branch: &str,
) -> Result<usize, ApiError> {
  let jobsets =
    repo::jobsets::list_all_for_project(&state.pool, project_id).await?;

  let mut triggered = 0;
  for jobset in &jobsets {
    if !jobset_accepts_source_trigger(jobset) {
      continue;
    }
    if !jobset_matches_branch(jobset.branch.as_deref(), pushed_branch) {
      continue;
    }
    match repo::evaluations::create(&state.pool, CreateEvaluation {
      jobset_id:      jobset.id,
      commit_hash:    commit.to_string(),
      pr_number:      None,
      pr_head_branch: None,
      pr_base_branch: None,
      pr_action:      None,
    })
    .await
    {
      Ok(_) => triggered += 1,
      Err(circus_common::CiError::Conflict(_)) => {},
      Err(e) => tracing::warn!("Failed to create evaluation: {e}"),
    }
  }

  Ok(triggered)
}

struct ChangeRequestEvaluation {
  commit:      String,
  number:      Option<i32>,
  head_branch: Option<String>,
  base_branch: Option<String>,
  action:      Option<String>,
}

async fn trigger_change_request_evaluations(
  state: &AppState,
  project_id: Uuid,
  input: &ChangeRequestEvaluation,
) -> Result<usize, ApiError> {
  let jobsets =
    repo::jobsets::list_all_for_project(&state.pool, project_id).await?;

  let base = input.base_branch.as_deref().unwrap_or("");

  let mut triggered = 0;
  for jobset in &jobsets {
    if !jobset_accepts_source_trigger(jobset) {
      continue;
    }
    if !jobset_matches_branch(jobset.branch.as_deref(), base) {
      continue;
    }
    match repo::evaluations::create(&state.pool, CreateEvaluation {
      jobset_id:      jobset.id,
      commit_hash:    input.commit.clone(),
      pr_number:      input.number,
      pr_head_branch: input.head_branch.clone(),
      pr_base_branch: input.base_branch.clone(),
      pr_action:      input.action.clone(),
    })
    .await
    {
      Ok(_) => triggered += 1,
      Err(circus_common::CiError::Conflict(_)) => {},
      Err(e) => tracing::warn!("Failed to create evaluation: {e}"),
    }
  }

  Ok(triggered)
}

/// Verify HMAC-SHA256 webhook signature.
/// The `secret` parameter is the raw webhook secret stored in DB.
fn verify_signature(secret: &str, body: &[u8], signature: &str) -> bool {
  use hmac::{Hmac, Mac};
  use sha2::Sha256;

  let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
    return false;
  };
  mac.update(body);

  let hex_sig = signature
    .strip_prefix("sha256=")
    .or_else(|| signature.strip_prefix("sha1="))
    .unwrap_or(signature);

  let Ok(sig_bytes) = hex::decode(hex_sig) else {
    return false;
  };

  mac.verify_slice(&sig_bytes).is_ok()
}

fn header_value<'a>(headers: &'a HeaderMap, name: &'static str) -> &'a str {
  headers
    .get(name)
    .and_then(|v| v.to_str().ok())
    .unwrap_or("")
}

fn webhook_not_configured(
  forge_name: &str,
) -> (StatusCode, Json<WebhookResponse>) {
  (
    StatusCode::NOT_FOUND,
    Json(WebhookResponse {
      accepted: false,
      message:  format!("No {forge_name} webhook configured for this project"),
    }),
  )
}

fn invalid_signature_response() -> (StatusCode, Json<WebhookResponse>) {
  (
    StatusCode::UNAUTHORIZED,
    Json(WebhookResponse {
      accepted: false,
      message:  "Invalid webhook signature".to_string(),
    }),
  )
}

fn branch_deletion_response() -> (StatusCode, Json<WebhookResponse>) {
  (
    StatusCode::OK,
    Json(WebhookResponse {
      accepted: true,
      message:  "Branch deletion event, skipping".to_string(),
    }),
  )
}

fn triggered_push_response(
  triggered: usize,
  commit: &str,
) -> (StatusCode, Json<WebhookResponse>) {
  (
    StatusCode::OK,
    Json(WebhookResponse {
      accepted: true,
      message:  format!(
        "Triggered {triggered} evaluations for commit {commit}"
      ),
    }),
  )
}
