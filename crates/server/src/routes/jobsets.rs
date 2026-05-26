use axum::{
  Json,
  Router,
  extract::{Path, State},
  routing::get,
};
use circus_common::{Jobset, JobsetInput, UpdateJobset, Validate};
use serde::Deserialize;
use uuid::Uuid;

use crate::{auth_middleware::RequireAdmin, error::ApiError, state::AppState};

async fn get_jobset(
  State(state): State<AppState>,
  Path((_project_id, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Jobset>, ApiError> {
  let jobset = circus_common::repo::jobsets::get(&state.pool, id)
    .await
    .map_err(ApiError)?;
  Ok(Json(jobset))
}

async fn update_jobset(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path((_project_id, id)): Path<(Uuid, Uuid)>,
  Json(input): Json<UpdateJobset>,
) -> Result<Json<Jobset>, ApiError> {
  input
    .validate()
    .map_err(|msg| ApiError(circus_common::CiError::Validation(msg)))?;
  let jobset = circus_common::repo::jobsets::update(&state.pool, id, input)
    .await
    .map_err(ApiError)?;
  Ok(Json(jobset))
}

async fn delete_jobset(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path((_project_id, id)): Path<(Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, ApiError> {
  circus_common::repo::jobsets::delete(&state.pool, id)
    .await
    .map_err(ApiError)?;
  Ok(Json(serde_json::json!({ "deleted": true })))
}

// Jobset input routes

async fn list_jobset_inputs(
  State(state): State<AppState>,
  Path((_project_id, jobset_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Vec<JobsetInput>>, ApiError> {
  let inputs =
    circus_common::repo::jobset_inputs::list_for_jobset(&state.pool, jobset_id)
      .await
      .map_err(ApiError)?;
  Ok(Json(inputs))
}

#[derive(Debug, Deserialize)]
struct CreateJobsetInputRequest {
  name:       String,
  input_type: String,
  value:      String,
  revision:   Option<String>,
}

async fn create_jobset_input(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path((_project_id, jobset_id)): Path<(Uuid, Uuid)>,
  Json(body): Json<CreateJobsetInputRequest>,
) -> Result<Json<JobsetInput>, ApiError> {
  let input = circus_common::repo::jobset_inputs::create(
    &state.pool,
    jobset_id,
    &body.name,
    &body.input_type,
    &body.value,
    body.revision.as_deref(),
  )
  .await
  .map_err(ApiError)?;
  Ok(Json(input))
}

async fn delete_jobset_input(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path((_project_id, _jobset_id, input_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<Json<serde_json::Value>, ApiError> {
  circus_common::repo::jobset_inputs::delete(&state.pool, input_id)
    .await
    .map_err(ApiError)?;
  Ok(Json(serde_json::json!({ "deleted": true })))
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route(
      "/projects/{project_id}/jobsets/{id}",
      get(get_jobset).put(update_jobset).delete(delete_jobset),
    )
    .route(
      "/projects/{project_id}/jobsets/{jobset_id}/inputs",
      get(list_jobset_inputs).post(create_jobset_input),
    )
    .route(
      "/projects/{project_id}/jobsets/{jobset_id}/inputs/{input_id}",
      axum::routing::delete(delete_jobset_input),
    )
}
