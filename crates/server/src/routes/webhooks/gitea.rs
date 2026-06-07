use axum::{
  Json,
  body::Bytes,
  extract::{Path, State},
  http::{HeaderMap, StatusCode},
};
use uuid::Uuid;

use super::{WebhookResponse, gitea_compatible};
use crate::{error::ApiError, state::AppState};

pub(super) const PROVIDER: gitea_compatible::SignedPushProvider =
  gitea_compatible::SignedPushProvider::new(
    "gitea",
    "Gitea",
    "x-gitea-signature",
  );

pub(super) async fn handle_webhook(
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  headers: HeaderMap,
  body: Bytes,
) -> Result<(StatusCode, Json<WebhookResponse>), ApiError> {
  gitea_compatible::handle_signed_push(
    PROVIDER, state, project_id, headers, body,
  )
  .await
}
