//! Hercules-compatible API for the Effect currently holding a task token.

use axum::{
  Router,
  body::Bytes,
  extract::{Path, State},
  http::{HeaderMap, StatusCode, header},
  response::IntoResponse,
  routing::get,
};
use circus_common::{
  CiError,
  repo::{
    effect_task_tokens::{self, EffectTask},
    state_files,
  },
};

use crate::{error::ApiError, state::AppState};

async fn authenticate(
  state: &AppState,
  headers: &HeaderMap,
  name: &str,
) -> Result<EffectTask, ApiError> {
  let token = headers
    .get(header::AUTHORIZATION)
    .and_then(|value| value.to_str().ok())
    .and_then(|value| value.strip_prefix("Bearer "))
    .ok_or_else(|| CiError::Unauthorized("missing task token".into()))?;
  let task = effect_task_tokens::active_task(&state.pool, token)
    .await?
    .ok_or_else(|| {
      CiError::Unauthorized("unknown or expired task token".into())
    })?;
  if !state_files::is_valid_name(name) {
    return Err(CiError::Validation("invalid state file name".into()).into());
  }
  Ok(task)
}

async fn get_state(
  State(state): State<AppState>,
  Path(name): Path<String>,
  headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
  let task = authenticate(&state, &headers, &name).await?;
  let data = state_files::get(&state.pool, task.project_id, &name)
    .await?
    .ok_or_else(|| CiError::NotFound(format!("state file {name}")))?;
  Ok(([(header::CONTENT_TYPE, "application/octet-stream")], data))
}

async fn put_state(
  State(state): State<AppState>,
  Path(name): Path<String>,
  headers: HeaderMap,
  body: Bytes,
) -> Result<StatusCode, ApiError> {
  let task = authenticate(&state, &headers, &name).await?;
  state_files::put(&state.pool, task.project_id, &name, &body, task.build_id)
    .await?;
  Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
  Router::new().route(
    "/api/v1/current-task/state/{name}/data",
    get(get_state).put(put_state),
  )
}
