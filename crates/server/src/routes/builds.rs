use axum::{
  Json,
  Router,
  body::Body,
  extract::{Path, Query, State},
  http::{Extensions, StatusCode},
  response::{IntoResponse, Response},
  routing::{get, post, put},
};
use fc_common::{
  Build,
  BuildProduct,
  BuildStep,
  PaginatedResponse,
  PaginationParams,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{auth_middleware::RequireRoles, error::ApiError, state::AppState};

fn check_role(
  extensions: &Extensions,
  allowed: &[&str],
) -> Result<(), ApiError> {
  RequireRoles::check(extensions, allowed)
    .map(|_| ())
    .map_err(|s| {
      ApiError(if s == StatusCode::FORBIDDEN {
        fc_common::CiError::Forbidden("Insufficient permissions".to_string())
      } else {
        fc_common::CiError::Unauthorized("Authentication required".to_string())
      })
    })
}

#[derive(Debug, Deserialize)]
struct ListBuildsParams {
  evaluation_id: Option<Uuid>,
  status:        Option<String>,
  system:        Option<String>,
  job_name:      Option<String>,
  limit:         Option<i64>,
  offset:        Option<i64>,
}

async fn list_builds(
  State(state): State<AppState>,
  Query(params): Query<ListBuildsParams>,
) -> Result<Json<PaginatedResponse<Build>>, ApiError> {
  let pagination = PaginationParams {
    limit:  params.limit,
    offset: params.offset,
  };
  let limit = pagination.limit();
  let offset = pagination.offset();
  let items = fc_common::repo::builds::list_filtered(
    &state.pool,
    params.evaluation_id,
    params.status.as_deref(),
    params.system.as_deref(),
    params.job_name.as_deref(),
    limit,
    offset,
  )
  .await
  .map_err(ApiError)?;
  let total = fc_common::repo::builds::count_filtered(
    &state.pool,
    params.evaluation_id,
    params.status.as_deref(),
    params.system.as_deref(),
    params.job_name.as_deref(),
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

async fn get_build(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Build>, ApiError> {
  let build = fc_common::repo::builds::get(&state.pool, id)
    .await
    .map_err(ApiError)?;
  Ok(Json(build))
}

async fn cancel_build(
  extensions: Extensions,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Vec<Build>>, ApiError> {
  check_role(&extensions, &["cancel-build"])?;
  let cancelled = fc_common::repo::builds::cancel_cascade(&state.pool, id)
    .await
    .map_err(ApiError)?;
  if cancelled.is_empty() {
    return Err(ApiError(fc_common::CiError::NotFound(
      "Build not found or not in a cancellable state".to_string(),
    )));
  }
  Ok(Json(cancelled))
}

async fn list_build_steps(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Vec<BuildStep>>, ApiError> {
  let steps = fc_common::repo::build_steps::list_for_build(&state.pool, id)
    .await
    .map_err(ApiError)?;
  Ok(Json(steps))
}

async fn list_build_products(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Vec<BuildProduct>>, ApiError> {
  let products =
    fc_common::repo::build_products::list_for_build(&state.pool, id)
      .await
      .map_err(ApiError)?;
  Ok(Json(products))
}

async fn build_stats(
  State(state): State<AppState>,
) -> Result<Json<fc_common::BuildStats>, ApiError> {
  let build_stats = fc_common::repo::builds::get_stats(&state.pool)
    .await
    .map_err(ApiError)?;
  Ok(Json(build_stats))
}

async fn recent_builds(
  State(state): State<AppState>,
) -> Result<Json<Vec<Build>>, ApiError> {
  let builds = fc_common::repo::builds::list_recent(&state.pool, 20)
    .await
    .map_err(ApiError)?;
  Ok(Json(builds))
}

async fn list_project_builds(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Vec<Build>>, ApiError> {
  let builds = fc_common::repo::builds::list_for_project(&state.pool, id)
    .await
    .map_err(ApiError)?;
  Ok(Json(builds))
}

async fn restart_build(
  extensions: Extensions,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Build>, ApiError> {
  check_role(&extensions, &["restart-jobs"])?;
  let build = fc_common::repo::builds::restart(&state.pool, id)
    .await
    .map_err(ApiError)?;

  tracing::info!(
      build_id = %id,
      job = %build.job_name,
      "Build restarted"
  );

  Ok(Json(build))
}

async fn bump_build(
  extensions: Extensions,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Build>, ApiError> {
  check_role(&extensions, &["bump-to-front"])?;
  let build = sqlx::query_as::<_, Build>(
    "UPDATE builds SET priority = priority + 10 WHERE id = $1 AND status = \
     'pending' RETURNING *",
  )
  .bind(id)
  .fetch_optional(&state.pool)
  .await
  .map_err(|e| ApiError(fc_common::CiError::Database(e)))?
  .ok_or_else(|| {
    ApiError(fc_common::CiError::Validation(
      "Build not found or not in pending state".to_string(),
    ))
  })?;

  Ok(Json(build))
}

async fn download_build_product(
  State(state): State<AppState>,
  Path((build_id, product_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, ApiError> {
  // Verify build exists
  let _build = fc_common::repo::builds::get(&state.pool, build_id)
    .await
    .map_err(ApiError)?;

  let product = fc_common::repo::build_products::get(&state.pool, product_id)
    .await
    .map_err(ApiError)?;

  if product.build_id != build_id {
    return Err(ApiError(fc_common::CiError::NotFound(
      "Product does not belong to this build".to_string(),
    )));
  }

  if !fc_common::validate::is_valid_store_path(&product.path) {
    return Err(ApiError(fc_common::CiError::Validation(
      "Invalid store path".to_string(),
    )));
  }

  if product.is_directory {
    // Stream as NAR using nix store dump-path
    let child = tokio::process::Command::new("nix")
      .args(["store", "dump-path", &product.path])
      .stdout(std::process::Stdio::piped())
      .stderr(std::process::Stdio::null())
      .spawn();

    let mut child = match child {
      Ok(c) => c,
      Err(e) => {
        return Err(ApiError(fc_common::CiError::Build(format!(
          "Failed to dump path: {e}"
        ))));
      },
    };

    let Some(stdout) = child.stdout.take() else {
      return Err(ApiError(fc_common::CiError::Build(
        "Failed to capture output".to_string(),
      )));
    };

    let stream = tokio_util::io::ReaderStream::new(stdout);
    let body = Body::from_stream(stream);

    let filename = product.path.rsplit('/').next().unwrap_or(&product.name);

    Ok(
      (
        StatusCode::OK,
        [
          ("content-type", "application/x-nix-nar"),
          (
            "content-disposition",
            &format!("attachment; filename=\"{filename}.nar\""),
          ),
        ],
        body,
      )
        .into_response(),
    )
  } else {
    // Serve file directly
    let file = tokio::fs::File::open(&product.path)
      .await
      .map_err(|e| ApiError(fc_common::CiError::Io(e)))?;

    let stream = tokio_util::io::ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let content_type = product
      .content_type
      .as_deref()
      .unwrap_or("application/octet-stream");
    let filename = product.path.rsplit('/').next().unwrap_or(&product.name);

    Ok(
      (
        StatusCode::OK,
        [
          ("content-type", content_type),
          (
            "content-disposition",
            &format!("attachment; filename=\"{filename}\""),
          ),
        ],
        body,
      )
        .into_response(),
    )
  }
}

async fn list_build_constituents(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Vec<Build>>, ApiError> {
  let build = fc_common::repo::builds::get(&state.pool, id)
    .await
    .map_err(ApiError)?;
  if !build.is_aggregate {
    return Err(ApiError(fc_common::CiError::Validation(
      "Build is not an aggregate build".into(),
    )));
  }
  let constituents =
    fc_common::repo::builds::list_constituents(&state.pool, id)
      .await
      .map_err(ApiError)?;
  Ok(Json(constituents))
}

async fn set_keep_flag(
  _auth: crate::auth_middleware::RequireAdmin,
  State(state): State<AppState>,
  Path((id, value)): Path<(Uuid, bool)>,
) -> Result<Json<Build>, ApiError> {
  let build = fc_common::repo::builds::set_keep(&state.pool, id, value)
    .await
    .map_err(ApiError)?;

  tracing::info!(
      build_id = %id,
      keep = value,
      "Build keep flag updated"
  );

  Ok(Json(build))
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/builds", get(list_builds))
    .route("/builds/stats", get(build_stats))
    .route("/builds/recent", get(recent_builds))
    .route("/builds/{id}", get(get_build))
    .route("/builds/{id}/cancel", post(cancel_build))
    .route("/builds/{id}/restart", post(restart_build))
    .route("/builds/{id}/bump", post(bump_build))
    .route("/builds/{id}/keep/{value}", put(set_keep_flag))
    .route("/builds/{id}/steps", get(list_build_steps))
    .route("/builds/{id}/products", get(list_build_products))
    .route(
      "/builds/{build_id}/products/{product_id}/download",
      get(download_build_product),
    )
    .route("/projects/{id}/builds", get(list_project_builds))
    .route("/builds/{id}/constituents", get(list_build_constituents))
}
