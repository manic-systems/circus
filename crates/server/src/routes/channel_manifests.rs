//! Channel manifest endpoints used by `nix-channel --update`.
//!
//! `nix-channel` fetches three small files when refreshing a channel:
//!
//! * `git-revision`     - the commit hash the channel currently points at,
//! * `binary-cache-url` - URL of the binary cache to pull NARs from,
//! * `store-paths.xz`   - xz-compressed newline-delimited list of every store
//!   path in the channel.
//!
//! `nixexprs.tar.xz` is intentionally not implemented - it requires the
//! evaluator to preserve the full expression tree, which is out of scope.
//!
//! All endpoints are public (no API key) since they are consumed by Nix
//! clients that have no way to supply credentials.

use std::io::Write;

use axum::{
  Router,
  body::Body,
  extract::{Path, State},
  http::StatusCode,
  response::{IntoResponse, Response},
  routing::get,
};

use crate::{error::ApiError, state::AppState};

async fn git_revision(
  State(state): State<AppState>,
  Path(name): Path<String>,
) -> Result<Response, ApiError> {
  let channel = fc_common::repo::channels::get_by_name(&state.pool, &name)
    .await
    .map_err(ApiError)?;
  let Some(eval_id) = channel.current_evaluation_id else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };
  let eval = fc_common::repo::evaluations::get(&state.pool, eval_id)
    .await
    .map_err(ApiError)?;

  Ok(
    (
      StatusCode::OK,
      [("content-type", "text/plain; charset=utf-8")],
      eval.commit_hash,
    )
      .into_response(),
  )
}

async fn binary_cache_url(
  State(state): State<AppState>,
  Path(name): Path<String>,
) -> Result<Response, ApiError> {
  // Verify the channel exists; otherwise this endpoint would happily echo
  // the cache URL for any name and pollute clients' channel state.
  let _ = fc_common::repo::channels::get_by_name(&state.pool, &name)
    .await
    .map_err(ApiError)?;

  let Some(url) = state.config.cache.cache_url.as_deref() else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };

  Ok(
    (
      StatusCode::OK,
      [("content-type", "text/plain; charset=utf-8")],
      url.to_string(),
    )
      .into_response(),
  )
}

async fn store_paths(
  State(state): State<AppState>,
  Path(name): Path<String>,
) -> Result<Response, ApiError> {
  let channel = fc_common::repo::channels::get_by_name(&state.pool, &name)
    .await
    .map_err(ApiError)?;
  let Some(eval_id) = channel.current_evaluation_id else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };

  let builds =
    fc_common::repo::builds::list_for_evaluation(&state.pool, eval_id)
      .await
      .map_err(ApiError)?;

  let mut paths: Vec<String> = Vec::with_capacity(builds.len() * 2);
  for build in &builds {
    if !build.status.is_success() {
      continue;
    }
    if let Some(p) = &build.build_output_path {
      paths.push(p.clone());
    }
    match fc_common::repo::build_products::list_for_build(&state.pool, build.id)
      .await
    {
      Ok(products) => {
        for product in products {
          paths.push(product.path);
        }
      },
      Err(e) => {
        tracing::warn!(
          build_id = %build.id,
          error = %e,
          "Failed to fetch build products for channel manifest; skipping",
        );
      },
    }
  }
  paths.sort();
  paths.dedup();

  let plain = paths.join("\n");
  let compressed = tokio::task::spawn_blocking(move || {
    let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 6);
    encoder.write_all(plain.as_bytes())?;
    encoder.finish()
  })
  .await
  .map_err(|e| {
    ApiError(fc_common::CiError::Build(format!("xz join error: {e}")))
  })?
  .map_err(|e| {
    ApiError(fc_common::CiError::Build(format!("xz encode failed: {e}")))
  })?;

  Ok(
    (
      StatusCode::OK,
      [("content-type", "application/x-xz")],
      Body::from(compressed),
    )
      .into_response(),
  )
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/channel/{name}/git-revision", get(git_revision))
    .route("/channel/{name}/binary-cache-url", get(binary_cache_url))
    .route("/channel/{name}/store-paths.xz", get(store_paths))
}
