use axum::{
  Json,
  body::Bytes,
  extract::{Path, State},
  http::{HeaderMap, StatusCode},
};
use circus_common::repo;
use serde::Deserialize;
use uuid::Uuid;

use super::{
  ChangeRequestEvaluation,
  WebhookResponse,
  branch_deletion_response,
  header_value,
  invalid_signature_response,
  is_deleted_commit,
  strip_branch_prefix,
  trace_webhook_repo,
  trigger_change_request_evaluations,
  trigger_push_evaluations,
  triggered_push_response,
  verify_signature,
  webhook_not_configured,
};
use crate::{error::ApiError, state::AppState};

const CONFIG_TYPE: &str = "github";
const DISPLAY_NAME: &str = "GitHub";
const SIGNATURE_HEADER: &str = "x-hub-signature-256";
const EVENT_HEADER: &str = "x-github-event";

#[derive(Debug, Deserialize)]
struct GithubPushPayload {
  #[serde(alias = "ref")]
  git_ref:    Option<String>,
  after:      Option<String>,
  repository: Option<GithubRepo>,
}

#[derive(Debug, Deserialize)]
struct GithubRepo {
  clone_url: Option<String>,
  html_url:  Option<String>,
}

#[derive(Debug, Deserialize)]
struct GithubPullRequestPayload {
  action:       Option<String>,
  number:       Option<u64>,
  pull_request: Option<GithubPullRequest>,
}

#[derive(Debug, Deserialize)]
struct GithubPullRequest {
  head:  Option<GithubPrRef>,
  base:  Option<GithubPrRef>,
  draft: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct GithubPrRef {
  sha:      Option<String>,
  #[serde(alias = "ref")]
  ref_name: Option<String>,
}

pub(super) async fn handle_webhook(
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  headers: HeaderMap,
  body: Bytes,
) -> Result<(StatusCode, Json<WebhookResponse>), ApiError> {
  let webhook_config = repo::webhook_configs::get_by_project_and_forge(
    &state.pool,
    project_id,
    CONFIG_TYPE,
    state.config.server.webhook_secret_encryption_key.as_deref(),
  )
  .await
  .map_err(ApiError)?;

  let Some(webhook_config) = webhook_config else {
    return Ok(webhook_not_configured(DISPLAY_NAME));
  };

  let Some(ref secret_hash) = webhook_config.secret_hash else {
    return Ok(webhook_not_configured(DISPLAY_NAME));
  };
  let signature = header_value(&headers, SIGNATURE_HEADER);
  if !verify_signature(secret_hash, &body, signature) {
    return Ok(invalid_signature_response());
  }

  let event_type = header_value(&headers, EVENT_HEADER);

  match event_type {
    "push" => handle_push(state, project_id, &body).await,
    "pull_request" => handle_pull_request(state, project_id, &body).await,
    _ => {
      Ok((
        StatusCode::OK,
        Json(WebhookResponse {
          accepted: true,
          message:  format!("Ignored GitHub event: {event_type}"),
        }),
      ))
    },
  }
}

async fn handle_push(
  state: AppState,
  project_id: Uuid,
  body: &[u8],
) -> Result<(StatusCode, Json<WebhookResponse>), ApiError> {
  let payload: GithubPushPayload =
    serde_json::from_slice(body).map_err(|e| {
      ApiError(circus_common::CiError::Validation(format!(
        "Invalid payload: {e}"
      )))
    })?;
  if let Some(repo) = payload.repository.as_ref() {
    trace_webhook_repo(
      CONFIG_TYPE,
      project_id,
      repo.clone_url.as_deref(),
      repo.html_url.as_deref(),
    );
  }

  let commit = payload.after.unwrap_or_default();
  if is_deleted_commit(&commit) {
    return Ok(branch_deletion_response());
  }

  let pushed_branch =
    payload.git_ref.as_deref().map_or("", strip_branch_prefix);

  let triggered =
    trigger_push_evaluations(&state, project_id, &commit, pushed_branch)
      .await?;

  Ok(triggered_push_response(triggered, &commit))
}

async fn handle_pull_request(
  state: AppState,
  project_id: Uuid,
  body: &[u8],
) -> Result<(StatusCode, Json<WebhookResponse>), ApiError> {
  let payload: GithubPullRequestPayload = serde_json::from_slice(body)
    .map_err(|e| {
      ApiError(circus_common::CiError::Validation(format!(
        "Invalid GitHub PR payload: {e}"
      )))
    })?;

  let action = payload.action.as_deref().unwrap_or("");

  if !matches!(action, "opened" | "synchronize" | "reopened") {
    return Ok((
      StatusCode::OK,
      Json(WebhookResponse {
        accepted: true,
        message:  format!("Ignored PR action: {action}"),
      }),
    ));
  }

  let Some(pr) = payload.pull_request else {
    return Ok((
      StatusCode::OK,
      Json(WebhookResponse {
        accepted: true,
        message:  "No pull request data, skipping".to_string(),
      }),
    ));
  };

  if pr.draft.unwrap_or(false) {
    return Ok((
      StatusCode::OK,
      Json(WebhookResponse {
        accepted: true,
        message:  "Draft pull request, skipping".to_string(),
      }),
    ));
  }

  let commit = pr
    .head
    .as_ref()
    .and_then(|h| h.sha.clone())
    .unwrap_or_default();
  if commit.is_empty() {
    return Ok((
      StatusCode::OK,
      Json(WebhookResponse {
        accepted: true,
        message:  "No commit in pull request, skipping".to_string(),
      }),
    ));
  }

  let pr_number = payload.number.map(|n| n as i32);
  let pr_head_branch = pr.head.as_ref().and_then(|h| h.ref_name.clone());
  let pr_base_branch = pr.base.as_ref().and_then(|b| b.ref_name.clone());
  let pr_action = Some(action.to_string());

  let triggered = trigger_change_request_evaluations(
    &state,
    project_id,
    &ChangeRequestEvaluation {
      commit:      commit.clone(),
      number:      pr_number,
      head_branch: pr_head_branch,
      base_branch: pr_base_branch,
      action:      pr_action,
    },
  )
  .await?;

  let pr_num = payload.number.unwrap_or(0);
  Ok((
    StatusCode::OK,
    Json(WebhookResponse {
      accepted: true,
      message:  format!(
        "Triggered {triggered} evaluations for PR #{pr_num} commit {commit}"
      ),
    }),
  ))
}

#[cfg(test)]
mod tests {
  #![expect(clippy::unwrap_used, reason = "Fine in tests")]

  use super::*;

  #[test]
  fn test_parse_github_push_payload() {
    let payload = r#"{
      "ref": "refs/heads/main",
      "after": "abc123def456789012345678901234567890abcd"
    }"#;

    let parsed: GithubPushPayload = serde_json::from_str(payload).unwrap();
    assert_eq!(
      parsed.after,
      Some("abc123def456789012345678901234567890abcd".to_string())
    );
    assert_eq!(parsed.git_ref, Some("refs/heads/main".to_string()));
  }

  #[test]
  fn test_parse_github_pr_payload() {
    let payload = r#"{
      "action": "opened",
      "number": 42,
      "pull_request": {
        "head": {"sha": "abc123", "ref": "feature-branch"},
        "base": {"sha": "def456", "ref": "main"},
        "draft": false
      }
    }"#;

    let parsed: GithubPullRequestPayload =
      serde_json::from_str(payload).unwrap();
    assert_eq!(parsed.action, Some("opened".to_string()));
    assert_eq!(parsed.number, Some(42));

    let pr = parsed.pull_request.unwrap();
    assert_eq!(pr.draft, Some(false));
    assert_eq!(
      pr.head.as_ref().and_then(|h| h.sha.clone()),
      Some("abc123".to_string())
    );
    assert_eq!(
      pr.head.as_ref().and_then(|h| h.ref_name.clone()),
      Some("feature-branch".to_string())
    );
  }

  #[test]
  fn test_parse_github_pr_draft() {
    let payload = r#"{
      "action": "opened",
      "number": 99,
      "pull_request": {
        "head": {"sha": "abc123", "ref": "draft-branch"},
        "base": {"sha": "def456", "ref": "main"},
        "draft": true
      }
    }"#;

    let parsed: GithubPullRequestPayload =
      serde_json::from_str(payload).unwrap();
    let pr = parsed.pull_request.unwrap();
    assert_eq!(pr.draft, Some(true));
  }
}
