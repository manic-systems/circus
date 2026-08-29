use axum::{
  Json,
  body::Bytes,
  extract::{Path, State},
  http::{HeaderMap, StatusCode},
};
use circus_common::{models::ForgeType, repo};
use serde::Deserialize;
use uuid::Uuid;

use super::{
  ChangeRequestEvaluation,
  PushedRef,
  WebhookResponse,
  branch_deletion_response,
  header_value,
  is_deleted_commit,
  parse_push_ref,
  trigger_change_request_evaluations,
  trigger_push_evaluations,
  triggered_push_response,
  webhook_not_configured,
};
use crate::{error::ApiError, state::AppState};

const CONFIG_TYPE: ForgeType = ForgeType::Gitlab;
const DISPLAY_NAME: &str = "GitLab";
const TOKEN_HEADER: &str = "x-gitlab-token";
const EVENT_HEADER: &str = "x-gitlab-event";

#[derive(Debug, Deserialize)]
struct GitLabPushPayload {
  #[serde(alias = "ref")]
  git_ref:      Option<String>,
  before:       Option<String>,
  after:        Option<String>,
  checkout_sha: Option<String>,
  project:      Option<GitLabProject>,
}

#[derive(Debug, Deserialize)]
struct GitLabProject {
  id:                  Option<i64>,
  path_with_namespace: Option<String>,
  web_url:             Option<String>,
  git_http_url:        Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitLabMergeRequestPayload {
  object_kind:       Option<String>,
  object_attributes: Option<GitLabMergeRequestAttributes>,
  project:           Option<GitLabProject>,
}

#[derive(Debug, Deserialize)]
struct GitLabMergeRequestAttributes {
  iid:              Option<u64>,
  action:           Option<String>,
  state:            Option<String>,
  source_branch:    Option<String>,
  target_branch:    Option<String>,
  last_commit:      Option<GitLabCommit>,
  oldrev:           Option<String>,
  work_in_progress: Option<bool>,
  draft:            Option<bool>,
}

#[derive(Debug, Deserialize)]
struct GitLabCommit {
  id: Option<String>,
}

pub(super) async fn handle_webhook(
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  headers: HeaderMap,
  body: Bytes,
) -> Result<(StatusCode, Json<WebhookResponse>), ApiError> {
  use subtle::ConstantTimeEq;

  let webhook_config = repo::webhook_configs::get_by_project_and_forge(
    &state.pool,
    project_id,
    CONFIG_TYPE,
    state.config.server.webhook_secret_encryption_key.as_deref(),
  )
  .await?;

  let Some(webhook_config) = webhook_config else {
    return Ok(webhook_not_configured(DISPLAY_NAME));
  };

  let Some(ref secret) = webhook_config.secret_hash else {
    return Ok(webhook_not_configured(DISPLAY_NAME));
  };
  let token = header_value(&headers, TOKEN_HEADER);
  let token_matches = token.len() == secret.len()
    && token.as_bytes().ct_eq(secret.as_bytes()).into();

  if !token_matches {
    return Ok((
      StatusCode::UNAUTHORIZED,
      Json(WebhookResponse {
        accepted: false,
        message:  "Invalid webhook token".to_string(),
      }),
    ));
  }

  let event_type = header_value(&headers, EVENT_HEADER);

  match event_type {
    "Push Hook" => handle_push(state, project_id, &body).await,
    "Merge Request Hook" => {
      handle_merge_request(state, project_id, &body).await
    },
    _ => {
      Ok((
        StatusCode::OK,
        Json(WebhookResponse {
          accepted: true,
          message:  format!("Ignored GitLab event: {event_type}"),
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
  let payload: GitLabPushPayload =
    serde_json::from_slice(body).map_err(|e| {
      ApiError(circus_common::CiError::Validation(format!(
        "Invalid GitLab push payload: {e}"
      )))
    })?;
  trace_project(project_id, payload.project.as_ref());

  let commit = payload.checkout_sha.or(payload.after).unwrap_or_default();

  if is_deleted_commit(&commit) {
    return Ok(branch_deletion_response());
  }

  let pushed_ref = payload
    .git_ref
    .as_deref()
    .map_or(PushedRef::Other(""), parse_push_ref);

  let triggered = trigger_push_evaluations(
    &state,
    project_id,
    &commit,
    payload.before.as_deref(),
    pushed_ref,
  )
  .await?;

  Ok(triggered_push_response(triggered, &commit))
}

async fn handle_merge_request(
  state: AppState,
  project_id: Uuid,
  body: &[u8],
) -> Result<(StatusCode, Json<WebhookResponse>), ApiError> {
  let payload: GitLabMergeRequestPayload = serde_json::from_slice(body)
    .map_err(|e| {
      ApiError(circus_common::CiError::Validation(format!(
        "Invalid GitLab MR payload: {e}"
      )))
    })?;
  trace_project(project_id, payload.project.as_ref());

  if let Some(kind) = payload.object_kind.as_deref()
    && kind != "merge_request"
  {
    return Ok((
      StatusCode::OK,
      Json(WebhookResponse {
        accepted: true,
        message:  format!("Ignored GitLab object kind: {kind}"),
      }),
    ));
  }

  let Some(attrs) = payload.object_attributes else {
    return Ok((
      StatusCode::OK,
      Json(WebhookResponse {
        accepted: true,
        message:  "No merge request attributes, skipping".to_string(),
      }),
    ));
  };

  if attrs.work_in_progress.unwrap_or(false) || attrs.draft.unwrap_or(false) {
    return Ok((
      StatusCode::OK,
      Json(WebhookResponse {
        accepted: true,
        message:  "Draft/WIP merge request, skipping".to_string(),
      }),
    ));
  }

  if let Some(state) = attrs.state.as_deref()
    && state != "opened"
  {
    return Ok((
      StatusCode::OK,
      Json(WebhookResponse {
        accepted: true,
        message:  format!("Ignored MR state: {state}"),
      }),
    ));
  }

  let action = attrs.action.as_deref().unwrap_or("");
  if !matches!(action, "open" | "update" | "reopen") {
    return Ok((
      StatusCode::OK,
      Json(WebhookResponse {
        accepted: true,
        message:  format!("Ignored MR action: {action}"),
      }),
    ));
  }

  let commit = attrs.last_commit.and_then(|c| c.id).unwrap_or_default();

  if commit.is_empty() {
    return Ok((
      StatusCode::OK,
      Json(WebhookResponse {
        accepted: true,
        message:  "No commit in merge request, skipping".to_string(),
      }),
    ));
  }

  let pr_number = attrs.iid.map(|n| n as i32);
  let pr_head_branch = attrs.source_branch.clone();
  let pr_base_branch = attrs.target_branch.clone();
  let pr_action = Some(action.to_string());

  let triggered = trigger_change_request_evaluations(
    &state,
    project_id,
    &ChangeRequestEvaluation {
      commit:          commit.clone(),
      previous_commit: attrs.oldrev,
      first:           action == "open",
      number:          pr_number,
      head_branch:     pr_head_branch,
      base_branch:     pr_base_branch,
      action:          pr_action,
    },
  )
  .await?;

  let mr_iid = pr_number.unwrap_or(0);
  Ok((
    StatusCode::OK,
    Json(WebhookResponse {
      accepted: true,
      message:  format!(
        "Triggered {triggered} evaluations for MR !{mr_iid} commit {commit}"
      ),
    }),
  ))
}

fn trace_project(project_id: Uuid, project: Option<&GitLabProject>) {
  if let Some(project) = project {
    tracing::debug!(
      %project_id,
      gitlab_project_id = ?project.id,
      path_with_namespace = ?project.path_with_namespace,
      web_url = ?project.web_url,
      git_http_url = ?project.git_http_url,
      "GitLab webhook payload project"
    );
  }
}

#[cfg(test)]
mod tests {
  #![expect(clippy::unwrap_used, reason = "Fine in tests")]

  use super::*;

  #[test]
  fn test_parse_gitlab_push_payload() {
    let payload = r#"{
      "ref": "refs/heads/main",
      "before": "abc123",
      "after": "abc123",
      "checkout_sha": "def456789012345678901234567890abcdef12"
    }"#;

    let parsed: GitLabPushPayload = serde_json::from_str(payload).unwrap();
    assert_eq!(
      parsed.checkout_sha,
      Some("def456789012345678901234567890abcdef12".to_string())
    );
    assert_eq!(parsed.before, Some("abc123".to_string()));
    assert_eq!(parsed.after, Some("abc123".to_string()));
  }

  #[test]
  fn test_parse_gitlab_mr_payload() {
    let payload = r#"{
      "object_kind": "merge_request",
      "object_attributes": {
        "iid": 123,
        "action": "open",
        "oldrev": "def456",
        "source_branch": "feature",
        "target_branch": "main",
        "last_commit": {"id": "abc123def456"},
        "draft": false,
        "work_in_progress": false
      }
    }"#;

    let parsed: GitLabMergeRequestPayload =
      serde_json::from_str(payload).unwrap();
    let attrs = parsed.object_attributes.unwrap();
    assert_eq!(attrs.iid, Some(123));
    assert_eq!(attrs.action, Some("open".to_string()));
    assert_eq!(attrs.oldrev, Some("def456".to_string()));
    assert_eq!(attrs.source_branch, Some("feature".to_string()));
    assert_eq!(attrs.target_branch, Some("main".to_string()));
    assert_eq!(attrs.draft, Some(false));
    assert_eq!(attrs.work_in_progress, Some(false));
  }

  #[test]
  fn test_parse_gitlab_mr_draft() {
    let payload = r#"{
      "object_kind": "merge_request",
      "object_attributes": {
        "iid": 999,
        "action": "open",
        "draft": true
      }
    }"#;

    let parsed: GitLabMergeRequestPayload =
      serde_json::from_str(payload).unwrap();
    let attrs = parsed.object_attributes.unwrap();
    assert_eq!(attrs.draft, Some(true));
  }

  #[test]
  fn test_parse_gitlab_mr_wip() {
    let payload = r#"{
      "object_kind": "merge_request",
      "object_attributes": {
        "iid": 888,
        "action": "open",
        "work_in_progress": true
      }
    }"#;

    let parsed: GitLabMergeRequestPayload =
      serde_json::from_str(payload).unwrap();
    let attrs = parsed.object_attributes.unwrap();
    assert_eq!(attrs.work_in_progress, Some(true));
  }
}
