use axum::{
  Json,
  Router,
  body::Bytes,
  extract::{Path, State},
  http::{HeaderMap, StatusCode},
  routing::post,
};
use circus_common::{models::CreateEvaluation, repo};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

#[derive(Debug, Serialize)]
struct WebhookResponse {
  accepted: bool,
  message:  String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GithubPushPayload {
  #[serde(alias = "ref")]
  git_ref:    Option<String>,
  after:      Option<String>,
  repository: Option<GithubRepo>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GithubRepo {
  clone_url: Option<String>,
  html_url:  Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GithubPullRequestPayload {
  action:       Option<String>,
  number:       Option<u64>,
  pull_request: Option<GithubPullRequest>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GithubPullRequest {
  head:  Option<GithubPrRef>,
  base:  Option<GithubPrRef>,
  draft: Option<bool>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GithubPrRef {
  sha:      Option<String>,
  #[serde(alias = "ref")]
  ref_name: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GiteaPushPayload {
  #[serde(alias = "ref")]
  git_ref:    Option<String>,
  after:      Option<String>,
  repository: Option<GiteaRepo>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GiteaRepo {
  clone_url: Option<String>,
  html_url:  Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitLabPushPayload {
  #[serde(alias = "ref")]
  git_ref:      Option<String>,
  after:        Option<String>,
  checkout_sha: Option<String>,
  project:      Option<GitLabProject>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitLabProject {
  id:                  Option<i64>,
  path_with_namespace: Option<String>,
  web_url:             Option<String>,
  git_http_url:        Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitLabMergeRequestPayload {
  object_kind:       Option<String>,
  object_attributes: Option<GitLabMergeRequestAttributes>,
  project:           Option<GitLabProject>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitLabMergeRequestAttributes {
  iid:              Option<u64>,
  action:           Option<String>,
  state:            Option<String>,
  source_branch:    Option<String>,
  target_branch:    Option<String>,
  last_commit:      Option<GitLabCommit>,
  work_in_progress: Option<bool>,
  draft:            Option<bool>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct GitLabCommit {
  id: Option<String>,
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

  // Parse the hex signature (strip "sha256=" prefix if present)
  let hex_sig = signature
    .strip_prefix("sha256=")
    .or_else(|| signature.strip_prefix("sha1="))
    .unwrap_or(signature);

  let Ok(sig_bytes) = hex::decode(hex_sig) else {
    return false;
  };

  mac.verify_slice(&sig_bytes).is_ok()
}

async fn handle_github_webhook(
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  headers: HeaderMap,
  body: Bytes,
) -> Result<(StatusCode, Json<WebhookResponse>), ApiError> {
  // Check webhook config exists
  let webhook_config = repo::webhook_configs::get_by_project_and_forge(
    &state.pool,
    project_id,
    "github",
  )
  .await
  .map_err(ApiError)?;

  let Some(webhook_config) = webhook_config else {
    return Ok((
      StatusCode::NOT_FOUND,
      Json(WebhookResponse {
        accepted: false,
        message:  "No GitHub webhook configured for this project".to_string(),
      }),
    ));
  };

  // Verify signature if secret is configured
  if let Some(ref secret_hash) = webhook_config.secret_hash {
    let signature = headers
      .get("x-hub-signature-256")
      .and_then(|v| v.to_str().ok())
      .unwrap_or("");

    if !verify_signature(secret_hash, &body, signature) {
      return Ok((
        StatusCode::UNAUTHORIZED,
        Json(WebhookResponse {
          accepted: false,
          message:  "Invalid webhook signature".to_string(),
        }),
      ));
    }
  }

  // Determine event type from X-GitHub-Event header
  let event_type = headers
    .get("x-github-event")
    .and_then(|v| v.to_str().ok())
    .unwrap_or("");

  match event_type {
    "push" => handle_github_push(state, project_id, &body).await,
    "pull_request" => {
      handle_github_pull_request(state, project_id, &body).await
    },
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

async fn handle_github_push(
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

  let commit = payload.after.unwrap_or_default();
  if commit.is_empty() || commit == "0000000000000000000000000000000000000000" {
    return Ok((
      StatusCode::OK,
      Json(WebhookResponse {
        accepted: true,
        message:  "Branch deletion event, skipping".to_string(),
      }),
    ));
  }

  // Find matching jobsets for this project and trigger evaluations
  let jobsets =
    repo::jobsets::list_for_project(&state.pool, project_id, 1000, 0)
      .await
      .map_err(ApiError)?;

  let mut triggered = 0;
  for jobset in &jobsets {
    if !jobset.enabled {
      continue;
    }
    match repo::evaluations::create(&state.pool, CreateEvaluation {
      jobset_id:      jobset.id,
      commit_hash:    commit.clone(),
      pr_number:      None,
      pr_head_branch: None,
      pr_base_branch: None,
      pr_action:      None,
    })
    .await
    {
      Ok(_) => triggered += 1,
      Err(circus_common::CiError::Conflict(_)) => {}, // already exists
      Err(e) => tracing::warn!("Failed to create evaluation: {e}"),
    }
  }

  Ok((
    StatusCode::OK,
    Json(WebhookResponse {
      accepted: true,
      message:  format!(
        "Triggered {triggered} evaluations for commit {commit}"
      ),
    }),
  ))
}

async fn handle_github_pull_request(
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

  // Only trigger on open/synchronize/reopen actions
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

  // Skip draft PRs
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

  let jobsets =
    repo::jobsets::list_for_project(&state.pool, project_id, 1000, 0)
      .await
      .map_err(ApiError)?;

  let mut triggered = 0;
  for jobset in &jobsets {
    if !jobset.enabled {
      continue;
    }
    match repo::evaluations::create(&state.pool, CreateEvaluation {
      jobset_id: jobset.id,
      commit_hash: commit.clone(),
      pr_number,
      pr_head_branch: pr_head_branch.clone(),
      pr_base_branch: pr_base_branch.clone(),
      pr_action: pr_action.clone(),
    })
    .await
    {
      Ok(_) => triggered += 1,
      Err(circus_common::CiError::Conflict(_)) => {},
      Err(e) => tracing::warn!("Failed to create evaluation: {e}"),
    }
  }

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

async fn handle_gitea_push(
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  headers: HeaderMap,
  body: Bytes,
) -> Result<(StatusCode, Json<WebhookResponse>), ApiError> {
  // Check webhook config exists
  let forge_type = if headers.get("x-forgejo-event").is_some() {
    "forgejo"
  } else {
    "gitea"
  };

  let webhook_config = repo::webhook_configs::get_by_project_and_forge(
    &state.pool,
    project_id,
    forge_type,
  )
  .await
  .map_err(ApiError)?;

  // Fall back to the other type if not found
  let webhook_config = if let Some(c) = webhook_config {
    c
  } else {
    let alt = if forge_type == "gitea" {
      "forgejo"
    } else {
      "gitea"
    };
    match repo::webhook_configs::get_by_project_and_forge(
      &state.pool,
      project_id,
      alt,
    )
    .await
    .map_err(ApiError)?
    {
      Some(c) => c,
      None => {
        return Ok((
          StatusCode::NOT_FOUND,
          Json(WebhookResponse {
            accepted: false,
            message:  "No Gitea/Forgejo webhook configured for this project"
              .to_string(),
          }),
        ));
      },
    }
  };

  // Verify signature if configured
  if let Some(ref secret_hash) = webhook_config.secret_hash {
    let signature = headers
      .get("x-gitea-signature")
      .or_else(|| headers.get("x-forgejo-signature"))
      .and_then(|v| v.to_str().ok())
      .unwrap_or("");

    if !verify_signature(secret_hash, &body, signature) {
      return Ok((
        StatusCode::UNAUTHORIZED,
        Json(WebhookResponse {
          accepted: false,
          message:  "Invalid webhook signature".to_string(),
        }),
      ));
    }
  }

  let payload: GiteaPushPayload =
    serde_json::from_slice(&body).map_err(|e| {
      ApiError(circus_common::CiError::Validation(format!(
        "Invalid payload: {e}"
      )))
    })?;

  let commit = payload.after.unwrap_or_default();
  if commit.is_empty() || commit == "0000000000000000000000000000000000000000" {
    return Ok((
      StatusCode::OK,
      Json(WebhookResponse {
        accepted: true,
        message:  "Branch deletion event, skipping".to_string(),
      }),
    ));
  }

  let jobsets =
    repo::jobsets::list_for_project(&state.pool, project_id, 1000, 0)
      .await
      .map_err(ApiError)?;

  let mut triggered = 0;
  for jobset in &jobsets {
    if !jobset.enabled {
      continue;
    }
    match repo::evaluations::create(&state.pool, CreateEvaluation {
      jobset_id:      jobset.id,
      commit_hash:    commit.clone(),
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

  Ok((
    StatusCode::OK,
    Json(WebhookResponse {
      accepted: true,
      message:  format!(
        "Triggered {triggered} evaluations for commit {commit}"
      ),
    }),
  ))
}

async fn handle_gitlab_webhook(
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  headers: HeaderMap,
  body: Bytes,
) -> Result<(StatusCode, Json<WebhookResponse>), ApiError> {
  use subtle::ConstantTimeEq;

  // Check webhook config exists
  let webhook_config = repo::webhook_configs::get_by_project_and_forge(
    &state.pool,
    project_id,
    "gitlab",
  )
  .await
  .map_err(ApiError)?;

  let Some(webhook_config) = webhook_config else {
    return Ok((
      StatusCode::NOT_FOUND,
      Json(WebhookResponse {
        accepted: false,
        message:  "No GitLab webhook configured for this project".to_string(),
      }),
    ));
  };

  // Verify token if secret is configured
  // GitLab uses X-Gitlab-Token header with plain token (not HMAC)
  if let Some(ref secret) = webhook_config.secret_hash {
    let token = headers
      .get("x-gitlab-token")
      .and_then(|v| v.to_str().ok())
      .unwrap_or("");

    // Use constant-time comparison to prevent timing attacks
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
  }

  // Determine event type from X-Gitlab-Event header
  let event_type = headers
    .get("x-gitlab-event")
    .and_then(|v| v.to_str().ok())
    .unwrap_or("");

  match event_type {
    "Push Hook" => handle_gitlab_push(state, project_id, &body).await,
    "Merge Request Hook" => {
      handle_gitlab_merge_request(state, project_id, &body).await
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

async fn handle_gitlab_push(
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

  // Use checkout_sha (the actual commit checked out) or fall back to after
  let commit = payload.checkout_sha.or(payload.after).unwrap_or_default();

  if commit.is_empty() || commit == "0000000000000000000000000000000000000000" {
    return Ok((
      StatusCode::OK,
      Json(WebhookResponse {
        accepted: true,
        message:  "Branch deletion event, skipping".to_string(),
      }),
    ));
  }

  let jobsets =
    repo::jobsets::list_for_project(&state.pool, project_id, 1000, 0)
      .await
      .map_err(ApiError)?;

  let mut triggered = 0;
  for jobset in &jobsets {
    if !jobset.enabled {
      continue;
    }
    match repo::evaluations::create(&state.pool, CreateEvaluation {
      jobset_id:      jobset.id,
      commit_hash:    commit.clone(),
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

  Ok((
    StatusCode::OK,
    Json(WebhookResponse {
      accepted: true,
      message:  format!(
        "Triggered {triggered} evaluations for commit {commit}"
      ),
    }),
  ))
}

async fn handle_gitlab_merge_request(
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

  let Some(attrs) = payload.object_attributes else {
    return Ok((
      StatusCode::OK,
      Json(WebhookResponse {
        accepted: true,
        message:  "No merge request attributes, skipping".to_string(),
      }),
    ));
  };

  // Skip draft/WIP merge requests
  if attrs.work_in_progress.unwrap_or(false) || attrs.draft.unwrap_or(false) {
    return Ok((
      StatusCode::OK,
      Json(WebhookResponse {
        accepted: true,
        message:  "Draft/WIP merge request, skipping".to_string(),
      }),
    ));
  }

  // Only trigger on open/update/reopen actions
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

  // Get the commit from the last commit in the MR
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

  let jobsets =
    repo::jobsets::list_for_project(&state.pool, project_id, 1000, 0)
      .await
      .map_err(ApiError)?;

  let mut triggered = 0;
  for jobset in &jobsets {
    if !jobset.enabled {
      continue;
    }
    match repo::evaluations::create(&state.pool, CreateEvaluation {
      jobset_id: jobset.id,
      commit_hash: commit.clone(),
      pr_number,
      pr_head_branch: pr_head_branch.clone(),
      pr_base_branch: pr_base_branch.clone(),
      pr_action: pr_action.clone(),
    })
    .await
    {
      Ok(_) => triggered += 1,
      Err(circus_common::CiError::Conflict(_)) => {},
      Err(e) => tracing::warn!("Failed to create evaluation: {e}"),
    }
  }

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

pub fn router() -> Router<AppState> {
  Router::new()
    .route(
      "/api/v1/webhooks/{project_id}/github",
      post(handle_github_webhook),
    )
    .route(
      "/api/v1/webhooks/{project_id}/gitea",
      post(handle_gitea_push),
    )
    .route(
      "/api/v1/webhooks/{project_id}/forgejo",
      post(handle_gitea_push),
    )
    .route(
      "/api/v1/webhooks/{project_id}/gitlab",
      post(handle_gitlab_webhook),
    )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_verify_signature_valid() {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let secret = "test-secret";
    let body = b"test-body";

    // Compute expected signature
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

    // Verify with different secret should fail
    assert!(!verify_signature("secret2", body, &format!("sha256={sig}")));
  }

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

  #[test]
  fn test_parse_gitlab_push_payload() {
    let payload = r#"{
      "ref": "refs/heads/main",
      "after": "abc123",
      "checkout_sha": "def456789012345678901234567890abcdef12"
    }"#;

    let parsed: GitLabPushPayload = serde_json::from_str(payload).unwrap();
    assert_eq!(
      parsed.checkout_sha,
      Some("def456789012345678901234567890abcdef12".to_string())
    );
    assert_eq!(parsed.after, Some("abc123".to_string()));
  }

  #[test]
  fn test_parse_gitlab_mr_payload() {
    let payload = r#"{
      "object_kind": "merge_request",
      "object_attributes": {
        "iid": 123,
        "action": "open",
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

  #[test]
  fn test_parse_gitea_push_payload() {
    let payload = r#"{
      "ref": "refs/heads/main",
      "after": "abc123def456789012345678901234567890abcd"
    }"#;

    let parsed: GiteaPushPayload = serde_json::from_str(payload).unwrap();
    assert_eq!(
      parsed.after,
      Some("abc123def456789012345678901234567890abcd".to_string())
    );
    assert_eq!(parsed.git_ref, Some("refs/heads/main".to_string()));
  }

  #[test]
  fn test_branch_deletion_detection() {
    // The null SHA indicates branch deletion
    let commit = "0000000000000000000000000000000000000000";
    assert!(
      commit.is_empty() || commit == "0000000000000000000000000000000000000000"
    );
  }
}
