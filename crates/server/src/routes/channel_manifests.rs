//! Channel manifest endpoints used by `nix-channel --update`.
//!
//! `nix-channel` fetches four small files when refreshing a channel:
//!
//! * `git-revision`     - the commit hash the channel currently points at,
//! * `binary-cache-url` - URL of the binary cache to pull NARs from,
//! * `store-paths.xz`   - xz-compressed newline-delimited list of every store
//!   path in the channel,
//! * `nixexprs.tar.xz`  - a tar.xz with a single `default.nix` exposing each
//!   succeeded build as a fake derivation so `nix-env -qa` and `nix-channel
//!   --update` work.
//!
//! All endpoints are public (no API key) since they are consumed by Nix
//! clients that have no way to supply credentials.

use async_compression::tokio::write::XzEncoder;
use axum::{
  Router,
  body::Body,
  extract::{Path, State},
  http::StatusCode,
  response::{IntoResponse, Response},
  routing::get,
};

use crate::{error::ApiError, state::AppState};

/// Encode `bytes` with xz (level 6 to match the historical default).
/// Async-only: uses `async-compression`'s tokio writer so we don't have
/// to spawn a blocking task or carry a second lzma backend.
async fn encode_xz(bytes: Vec<u8>) -> std::io::Result<Vec<u8>> {
  use tokio::io::AsyncWriteExt as _;
  let mut enc = XzEncoder::with_quality(
    Vec::with_capacity(bytes.len()),
    async_compression::Level::Precise(6),
  );
  enc.write_all(&bytes).await?;
  enc.shutdown().await?;
  Ok(enc.into_inner())
}

async fn git_revision(
  State(state): State<AppState>,
  Path(name): Path<String>,
) -> Result<Response, ApiError> {
  let channel = circus_common::repo::channels::get_by_name(&state.pool, &name)
    .await
    .map_err(ApiError)?;
  let Some(eval_id) = channel.current_evaluation_id else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };
  let eval = circus_common::repo::evaluations::get(&state.pool, eval_id)
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
  let _ = circus_common::repo::channels::get_by_name(&state.pool, &name)
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
  let channel = circus_common::repo::channels::get_by_name(&state.pool, &name)
    .await
    .map_err(ApiError)?;
  let Some(eval_id) = channel.current_evaluation_id else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };

  let builds =
    circus_common::repo::builds::list_for_evaluation(&state.pool, eval_id)
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
    match circus_common::repo::build_products::list_for_build(
      &state.pool,
      build.id,
    )
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
  let compressed = encode_xz(plain.into_bytes()).await.map_err(|e| {
    ApiError(circus_common::CiError::Build(format!(
      "xz encode failed: {e}"
    )))
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

async fn nixexprs(
  State(state): State<AppState>,
  Path(name): Path<String>,
) -> Result<Response, ApiError> {
  let channel = circus_common::repo::channels::get_by_name(&state.pool, &name)
    .await
    .map_err(ApiError)?;
  let Some(eval_id) = channel.current_evaluation_id else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };

  let xz_data = crate::routes::channels::build_nixexprs_tarball(
    &state.pool,
    &channel.name,
    eval_id,
  )
  .await?;

  Ok(
    (
      StatusCode::OK,
      [
        ("content-type", "application/x-xz"),
        (
          "content-disposition",
          "attachment; filename=\"nixexprs.tar.xz\"",
        ),
      ],
      Body::from(xz_data),
    )
      .into_response(),
  )
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/channel/{name}/git-revision", get(git_revision))
    .route("/channel/{name}/binary-cache-url", get(binary_cache_url))
    .route("/channel/{name}/store-paths.xz", get(store_paths))
    .route("/channel/{name}/nixexprs.tar.xz", get(nixexprs))
}
