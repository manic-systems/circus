use axum::{
  Json,
  body::Bytes,
  http::{HeaderMap, StatusCode},
};
use circus_common::repo;
use serde::Deserialize;
use uuid::Uuid;

use super::{
  WebhookResponse,
  branch_deletion_response,
  header_value,
  invalid_signature_response,
  is_deleted_commit,
  strip_branch_prefix,
  trace_webhook_repo,
  trigger_push_evaluations,
  triggered_push_response,
  verify_signature,
  webhook_not_configured,
};
use crate::{error::ApiError, state::AppState};

#[derive(Clone, Copy)]
pub(super) struct SignedPushProvider {
  config_type:      &'static str,
  display_name:     &'static str,
  signature_header: &'static str,
}

impl SignedPushProvider {
  pub(super) const fn new(
    config_type: &'static str,
    display_name: &'static str,
    signature_header: &'static str,
  ) -> Self {
    Self {
      config_type,
      display_name,
      signature_header,
    }
  }

  pub(super) fn is_signature_valid(
    self,
    headers: &HeaderMap,
    body: &[u8],
    secret: &str,
  ) -> bool {
    let signature = header_value(headers, self.signature_header);
    verify_signature(secret, body, signature)
  }
}

#[derive(Debug, Deserialize)]
struct GiteaCompatiblePushPayload {
  #[serde(alias = "ref")]
  git_ref:    Option<String>,
  after:      Option<String>,
  repository: Option<GiteaCompatibleRepo>,
}

#[derive(Debug, Deserialize)]
struct GiteaCompatibleRepo {
  clone_url: Option<String>,
  html_url:  Option<String>,
}

pub(super) async fn handle_signed_push(
  provider: SignedPushProvider,
  state: AppState,
  project_id: Uuid,
  headers: HeaderMap,
  body: Bytes,
) -> Result<(StatusCode, Json<WebhookResponse>), ApiError> {
  let webhook_config = repo::webhook_configs::get_by_project_and_forge(
    &state.pool,
    project_id,
    provider.config_type,
  )
  .await
  .map_err(ApiError)?;

  let Some(webhook_config) = webhook_config else {
    return Ok(webhook_not_configured(provider.display_name));
  };

  if let Some(ref secret_hash) = webhook_config.secret_hash
    && !provider.is_signature_valid(&headers, &body, secret_hash)
  {
    return Ok(invalid_signature_response());
  }

  process_push(state, project_id, provider.config_type, body).await
}

async fn process_push(
  state: AppState,
  project_id: Uuid,
  forge_type: &'static str,
  body: Bytes,
) -> Result<(StatusCode, Json<WebhookResponse>), ApiError> {
  let payload: GiteaCompatiblePushPayload = serde_json::from_slice(&body)
    .map_err(|e| {
      ApiError(circus_common::CiError::Validation(format!(
        "Invalid payload: {e}"
      )))
    })?;
  if let Some(repo) = payload.repository.as_ref() {
    trace_webhook_repo(
      forge_type,
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

#[cfg(test)]
mod tests {
  #![expect(clippy::unwrap_used, reason = "Fine in tests")]

  use super::*;

  #[test]
  fn test_parse_gitea_compatible_push_payload() {
    let payload = r#"{
      "ref": "refs/heads/main",
      "after": "abc123def456789012345678901234567890abcd"
    }"#;

    let parsed: GiteaCompatiblePushPayload =
      serde_json::from_str(payload).unwrap();
    assert_eq!(
      parsed.after,
      Some("abc123def456789012345678901234567890abcd".to_string())
    );
    assert_eq!(parsed.git_ref, Some("refs/heads/main".to_string()));
  }
}
