use std::collections::HashMap;

use axum::{
  Json,
  Router,
  extract::{Path, Query, State},
  http::Extensions,
  routing::{get, post},
};
use circus_common::{
  CreateEvaluation,
  Evaluation,
  PaginatedResponse,
  PaginationParams,
  Validate,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{auth_middleware::RequireRoles, error::ApiError, state::AppState};

#[derive(Debug, Deserialize)]
struct ListEvaluationsParams {
  jobset_id: Option<Uuid>,
  status:    Option<String>,
  limit:     Option<i64>,
  offset:    Option<i64>,
}

async fn list_evaluations(
  State(state): State<AppState>,
  Query(params): Query<ListEvaluationsParams>,
) -> Result<Json<PaginatedResponse<Evaluation>>, ApiError> {
  let pagination = PaginationParams {
    limit:  params.limit,
    offset: params.offset,
  };
  let limit = pagination.limit();
  let offset = pagination.offset();
  let items = circus_common::repo::evaluations::list_filtered(
    &state.pool,
    params.jobset_id,
    params.status.as_deref(),
    limit,
    offset,
  )
  .await
  .map_err(ApiError)?;
  let total = circus_common::repo::evaluations::count_filtered(
    &state.pool,
    params.jobset_id,
    params.status.as_deref(),
  )
  .await
  .map_err(ApiError)?;
  Ok(Json(PaginatedResponse {
    items,
    total,
    limit,
    offset,
  }))
}

async fn get_evaluation(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Evaluation>, ApiError> {
  let evaluation = circus_common::repo::evaluations::get(&state.pool, id)
    .await
    .map_err(ApiError)?;
  Ok(Json(evaluation))
}

async fn trigger_evaluation(
  extensions: Extensions,
  State(state): State<AppState>,
  Json(input): Json<CreateEvaluation>,
) -> Result<Json<Evaluation>, ApiError> {
  RequireRoles::check(&extensions, &["eval-jobset"]).map_err(|s| {
    ApiError(if s == axum::http::StatusCode::FORBIDDEN {
      circus_common::CiError::Forbidden("Insufficient permissions".to_string())
    } else {
      circus_common::CiError::Unauthorized(
        "Authentication required".to_string(),
      )
    })
  })?;
  input
    .validate()
    .map_err(|msg| ApiError(circus_common::CiError::Validation(msg)))?;
  let evaluation = circus_common::repo::evaluations::create(&state.pool, input)
    .await
    .map_err(ApiError)?;
  Ok(Json(evaluation))
}

#[derive(Debug, Deserialize)]
struct CompareParams {
  to: Uuid,
}

#[derive(Debug, Serialize)]
struct EvalComparison {
  from_id:         Uuid,
  to_id:           Uuid,
  new_jobs:        Vec<JobDiff>,
  removed_jobs:    Vec<JobDiff>,
  changed_jobs:    Vec<JobChange>,
  unchanged_count: usize,
}

#[derive(Debug, Serialize)]
struct JobDiff {
  job_name: String,
  system:   Option<String>,
  drv_path: String,
  status:   String,
}

#[derive(Debug, Serialize)]
struct JobChange {
  job_name:   String,
  system:     Option<String>,
  old_drv:    String,
  new_drv:    String,
  old_status: String,
  new_status: String,
}

async fn compare_evaluations(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  Query(params): Query<CompareParams>,
) -> Result<Json<EvalComparison>, ApiError> {
  // Verify both evaluations exist
  let _from_eval = circus_common::repo::evaluations::get(&state.pool, id)
    .await
    .map_err(ApiError)?;
  let _to_eval = circus_common::repo::evaluations::get(&state.pool, params.to)
    .await
    .map_err(ApiError)?;

  let from_builds =
    circus_common::repo::builds::list_for_evaluation(&state.pool, id)
      .await
      .map_err(ApiError)?;
  let to_builds =
    circus_common::repo::builds::list_for_evaluation(&state.pool, params.to)
      .await
      .map_err(ApiError)?;

  let from_map: HashMap<&str, &circus_common::Build> = from_builds
    .iter()
    .map(|b| (b.job_name.as_str(), b))
    .collect();
  let to_map: HashMap<&str, &circus_common::Build> =
    to_builds.iter().map(|b| (b.job_name.as_str(), b)).collect();

  let mut new_jobs = Vec::new();
  let mut removed_jobs = Vec::new();
  let mut changed_jobs = Vec::new();
  let mut unchanged_count = 0;

  // Jobs in `to` but not in `from` are new
  for (name, build) in &to_map {
    if !from_map.contains_key(name) {
      new_jobs.push(JobDiff {
        job_name: name.to_string(),
        system:   build.system.clone(),
        drv_path: build.drv_path.clone(),
        status:   format!("{:?}", build.status),
      });
    }
  }

  // Jobs in `from` but not in `to` are removed
  for (name, build) in &from_map {
    if !to_map.contains_key(name) {
      removed_jobs.push(JobDiff {
        job_name: name.to_string(),
        system:   build.system.clone(),
        drv_path: build.drv_path.clone(),
        status:   format!("{:?}", build.status),
      });
    }
  }

  // Jobs in both: compare derivation paths
  for (name, from_build) in &from_map {
    if let Some(to_build) = to_map.get(name) {
      if from_build.drv_path == to_build.drv_path {
        unchanged_count += 1;
      } else {
        changed_jobs.push(JobChange {
          job_name:   name.to_string(),
          system:     to_build.system.clone(),
          old_drv:    from_build.drv_path.clone(),
          new_drv:    to_build.drv_path.clone(),
          old_status: format!("{:?}", from_build.status),
          new_status: format!("{:?}", to_build.status),
        });
      }
    }
  }

  Ok(Json(EvalComparison {
    from_id: id,
    to_id: params.to,
    new_jobs,
    removed_jobs,
    changed_jobs,
    unchanged_count,
  }))
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/evaluations", get(list_evaluations))
    .route("/evaluations/{id}", get(get_evaluation))
    .route("/evaluations/{id}/compare", get(compare_evaluations))
    .route("/evaluations/trigger", post(trigger_evaluation))
}
