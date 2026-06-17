use axum::{
  Json,
  Router,
  body::Body,
  extract::{Path, Query, State},
  http::{Extensions, HeaderValue, StatusCode, header},
  response::{IntoResponse, Response},
  routing::{get, post, put},
};
use circus_common::{
  Build,
  BuildProduct,
  BuildStep,
  PaginatedResponse,
  PaginationParams,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
  error::ApiError,
  permissions::{self, Permission},
  state::AppState,
};

#[derive(Debug, Deserialize)]
struct ListBuildsParams {
  evaluation_id: Option<Uuid>,
  #[serde(
    default,
    deserialize_with = "crate::routes::serde_util::empty_string_as_none"
  )]
  status:        Option<String>,
  #[serde(
    default,
    deserialize_with = "crate::routes::serde_util::empty_string_as_none"
  )]
  system:        Option<String>,
  #[serde(
    default,
    deserialize_with = "crate::routes::serde_util::empty_string_as_none"
  )]
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
  let status = params.status.as_deref();
  let system = params.system.as_deref();
  let job_name = params.job_name.as_deref();
  let items = circus_common::repo::builds::list_filtered(
    &state.pool,
    params.evaluation_id,
    status,
    system,
    job_name,
    limit,
    offset,
  )
  .await?;
  let total = circus_common::repo::builds::count_filtered(
    &state.pool,
    params.evaluation_id,
    status,
    system,
    job_name,
  )
  .await?;
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
  let build = circus_common::repo::builds::get(&state.pool, id).await?;
  Ok(Json(build))
}

async fn cancel_build(
  extensions: Extensions,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Vec<Build>>, ApiError> {
  permissions::require_api(&extensions, Permission::CancelBuild)?;
  let cancelled =
    circus_common::repo::builds::cancel_cascade(&state.pool, id).await?;
  if cancelled.is_empty() {
    return Err(ApiError(circus_common::CiError::NotFound(
      "Build not found or not in a cancellable state".to_string(),
    )));
  }
  Ok(Json(cancelled))
}

async fn list_build_steps(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Vec<BuildStep>>, ApiError> {
  let steps =
    circus_common::repo::build_steps::list_for_build(&state.pool, id).await?;
  Ok(Json(steps))
}

async fn list_build_products(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Vec<BuildProduct>>, ApiError> {
  let products =
    circus_common::repo::build_products::list_for_build(&state.pool, id)
      .await?;
  Ok(Json(products))
}

async fn list_build_dependencies(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Vec<Build>>, ApiError> {
  let deps = circus_common::repo::build_dependencies::list_dependency_builds(
    &state.pool,
    id,
  )
  .await?;
  Ok(Json(deps))
}

async fn list_build_dependents(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Vec<Build>>, ApiError> {
  let dependents =
    circus_common::repo::build_dependencies::list_dependent_builds(
      &state.pool,
      id,
    )
    .await?;
  Ok(Json(dependents))
}

async fn build_stats(
  State(state): State<AppState>,
) -> Result<Json<circus_common::BuildStats>, ApiError> {
  let build_stats = circus_common::repo::builds::get_stats(&state.pool).await?;
  Ok(Json(build_stats))
}

async fn recent_builds(
  State(state): State<AppState>,
) -> Result<Json<Vec<Build>>, ApiError> {
  let builds =
    circus_common::repo::builds::list_recent(&state.pool, 20).await?;
  Ok(Json(builds))
}

async fn list_project_builds(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Vec<Build>>, ApiError> {
  let builds =
    circus_common::repo::builds::list_for_project(&state.pool, id).await?;
  Ok(Json(builds))
}

async fn restart_build(
  extensions: Extensions,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Build>, ApiError> {
  permissions::require_api(&extensions, Permission::RestartJobs)?;
  let build = circus_common::repo::builds::restart(&state.pool, id).await?;

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
  permissions::require_api(&extensions, Permission::BumpToFront)?;
  let build = circus_common::repo::builds::bump_priority(&state.pool, id, 10)
    .await?
    .ok_or_else(|| {
      ApiError(circus_common::CiError::Validation(
        "Build not found or no longer pending".to_string(),
      ))
    })?;

  Ok(Json(build))
}

async fn download_build_product(
  State(state): State<AppState>,
  Path((build_id, product_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, ApiError> {
  // Verify build exists
  let _build = circus_common::repo::builds::get(&state.pool, build_id).await?;

  let product =
    circus_common::repo::build_products::get(&state.pool, product_id).await?;

  if product.build_id != build_id {
    return Err(ApiError(circus_common::CiError::NotFound(
      "Product does not belong to this build".to_string(),
    )));
  }

  let store_dir = state.config.nix.store_dir.to_string_lossy();
  if !circus_nix::StorePath::is_valid(
    &product.path,
    store_dir.trim_end_matches('/'),
  ) {
    return Err(ApiError(circus_common::CiError::Validation(
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
        return Err(ApiError(circus_common::CiError::Build(format!(
          "Failed to dump path: {e}"
        ))));
      },
    };

    let Some(stdout) = child.stdout.take() else {
      return Err(ApiError(circus_common::CiError::Build(
        "Failed to capture output".to_string(),
      )));
    };

    let stream = tokio_util::io::ReaderStream::new(stdout);
    let body = Body::from_stream(stream);

    let disposition = HeaderValue::from_str(&format!(
      "attachment; filename=\"{product_id}.nar\""
    ))
    .unwrap_or_else(|_| HeaderValue::from_static("attachment"));

    let mut response = (StatusCode::OK, body).into_response();
    response.headers_mut().insert(
      header::CONTENT_TYPE,
      HeaderValue::from_static("application/x-nix-nar"),
    );
    response
      .headers_mut()
      .insert(header::CONTENT_DISPOSITION, disposition);
    Ok(response)
  } else {
    // Serve file directly
    let file = tokio::fs::File::open(&product.path)
      .await
      .map_err(|e| ApiError(circus_common::CiError::Io(e)))?;

    let stream = tokio_util::io::ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let disposition =
      HeaderValue::from_str(&format!("attachment; filename=\"{product_id}\""))
        .unwrap_or_else(|_| HeaderValue::from_static("attachment"));

    let mut response = (StatusCode::OK, body).into_response();
    response.headers_mut().insert(
      header::CONTENT_TYPE,
      HeaderValue::from_static("application/octet-stream"),
    );
    response
      .headers_mut()
      .insert(header::CONTENT_DISPOSITION, disposition);
    Ok(response)
  }
}

async fn list_build_constituents(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Vec<Build>>, ApiError> {
  let build = circus_common::repo::builds::get(&state.pool, id).await?;
  if !build.is_aggregate {
    return Err(ApiError(circus_common::CiError::Validation(
      "Build is not an aggregate build".into(),
    )));
  }
  let constituents =
    circus_common::repo::builds::list_constituents(&state.pool, id).await?;
  Ok(Json(constituents))
}

async fn set_keep_flag(
  _auth: crate::auth_middleware::RequireAdmin,
  State(state): State<AppState>,
  Path((id, value)): Path<(Uuid, bool)>,
) -> Result<Json<Build>, ApiError> {
  let build =
    circus_common::repo::builds::set_keep(&state.pool, id, value).await?;

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
    .route("/builds/{id}/dependencies", get(list_build_dependencies))
    .route("/builds/{id}/dependents", get(list_build_dependents))
    .route(
      "/builds/{build_id}/products/{product_id}/download",
      get(download_build_product),
    )
    .route("/projects/{id}/builds", get(list_project_builds))
    .route("/builds/{id}/constituents", get(list_build_constituents))
}
