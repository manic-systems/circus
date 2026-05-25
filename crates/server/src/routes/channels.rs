use std::fmt::Write;

use axum::{
  Json,
  Router,
  body::Body,
  extract::{Path, State},
  http::StatusCode,
  response::{IntoResponse, Response},
  routing::{get, post},
};
use circus_common::{
  Validate,
  models::{BuildStatus, Channel, CreateChannel},
};
use uuid::Uuid;

use crate::{auth_middleware::RequireAdmin, error::ApiError, state::AppState};

async fn list_channels(
  State(state): State<AppState>,
) -> Result<Json<Vec<Channel>>, ApiError> {
  let channels = circus_common::repo::channels::list_all(&state.pool)
    .await
    .map_err(ApiError)?;
  Ok(Json(channels))
}

async fn list_project_channels(
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
) -> Result<Json<Vec<Channel>>, ApiError> {
  let channels =
    circus_common::repo::channels::list_for_project(&state.pool, project_id)
      .await
      .map_err(ApiError)?;
  Ok(Json(channels))
}

async fn get_channel(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Channel>, ApiError> {
  let channel = circus_common::repo::channels::get(&state.pool, id)
    .await
    .map_err(ApiError)?;
  Ok(Json(channel))
}

async fn create_channel(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Json(input): Json<CreateChannel>,
) -> Result<Json<Channel>, ApiError> {
  input
    .validate()
    .map_err(|msg| ApiError(circus_common::CiError::Validation(msg)))?;
  let jobset_id = input.jobset_id;
  let channel = circus_common::repo::channels::create(&state.pool, input)
    .await
    .map_err(ApiError)?;

  // Catch-up: if the jobset already has a completed evaluation, promote now
  if let Ok(Some(eval)) =
    circus_common::repo::evaluations::get_latest(&state.pool, jobset_id).await
    && eval.status == circus_common::models::EvaluationStatus::Completed
    && let Err(e) = circus_common::repo::channels::auto_promote_if_complete(
      &state.pool,
      jobset_id,
      eval.id,
    )
    .await
  {
    tracing::warn!(jobset_id = %jobset_id, "Failed to auto-promote channel: {e}");
  }

  // Re-fetch to include any promotion
  let channel = circus_common::repo::channels::get(&state.pool, channel.id)
    .await
    .map_err(ApiError)?;
  Ok(Json(channel))
}

async fn delete_channel(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
  circus_common::repo::channels::delete(&state.pool, id)
    .await
    .map_err(ApiError)?;
  Ok(Json(serde_json::json!({"deleted": true})))
}

async fn promote_channel(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path((channel_id, eval_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Channel>, ApiError> {
  let channel =
    circus_common::repo::channels::promote(&state.pool, channel_id, eval_id)
      .await
      .map_err(ApiError)?;
  Ok(Json(channel))
}

/// Generate and serve `nixexprs.tar.xz` for Nix channel compatibility.
/// Contains a `default.nix` with fake derivations pointing at store paths,
/// enabling `nix-channel --add <url>`.
async fn nixexprs_tarball(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
  let channel = circus_common::repo::channels::get(&state.pool, id)
    .await
    .map_err(ApiError)?;

  let evaluation_id = channel.current_evaluation_id.ok_or_else(|| {
    ApiError(circus_common::CiError::NotFound(
      "Channel has no current evaluation".to_string(),
    ))
  })?;

  let builds = circus_common::repo::builds::list_for_evaluation(
    &state.pool,
    evaluation_id,
  )
  .await
  .map_err(ApiError)?;

  let succeeded: Vec<_> = builds
    .iter()
    .filter(|b| b.status == BuildStatus::Succeeded)
    .collect();

  if succeeded.is_empty() {
    return Err(ApiError(circus_common::CiError::NotFound(
      "No succeeded builds in current evaluation".to_string(),
    )));
  }

  // Generate default.nix
  let approx_size = 256 + succeeded.len() * 200;
  let mut nix_src = String::with_capacity(approx_size);
  let _ = writeln!(nix_src, "{{ system ? builtins.currentSystem }}:");
  let _ = writeln!(nix_src, "let");
  let _ = writeln!(nix_src, "  mkFakeDerivation = attrs:");
  let _ = writeln!(
    nix_src,
    "    let d = derivation (attrs // {{ builder = \"builtin:fetchurl\"; \
     preferLocalBuild = true; }});"
  );
  let _ = writeln!(
    nix_src,
    "    in d // {{ type = \"derivation\"; inherit (d) outPath drvPath name \
     system; outputSpecified = true; }};"
  );
  let _ = writeln!(nix_src, "in {{");

  for build in &succeeded {
    let Some(output_path) = &build.build_output_path else {
      continue;
    };
    let system = build.system.as_deref().unwrap_or("x86_64-linux");
    // Sanitize job_name for use as a Nix attribute (replace dots/slashes)
    let attr_name = build.job_name.replace(['.', '/'], "-");
    let _ = writeln!(
      nix_src,
      "  \"{attr_name}\" = mkFakeDerivation {{ name = \"{}\"; system = \
       \"{system}\"; outPath = \"{output_path}\"; }};",
      build.job_name.replace('"', "\\\""),
    );
  }

  let _ = writeln!(nix_src, "}}");

  // Build tar.xz archive in memory
  let xz_data =
    tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
      let mut xz_buf = Vec::new();
      {
        let xz_writer = xz2::write::XzEncoder::new(&mut xz_buf, 6);
        let mut tar_builder = tar::Builder::new(xz_writer);

        let nix_bytes = nix_src.as_bytes();
        let mut header = tar::Header::new_gnu();
        header.set_size(nix_bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();

        tar_builder
          .append_data(&mut header, "default.nix", nix_bytes)
          .map_err(|e| format!("Failed to append to tar: {e}"))?;

        let xz_writer = tar_builder
          .into_inner()
          .map_err(|e| format!("Failed to finish tar: {e}"))?;
        xz_writer
          .finish()
          .map_err(|e| format!("Failed to finish xz: {e}"))?;
      }
      Ok(xz_buf)
    })
    .await
    .map_err(|e| {
      ApiError(circus_common::CiError::Build(format!(
        "Task join error: {e}"
      )))
    })?
    .map_err(|e| ApiError(circus_common::CiError::Build(e)))?;

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
    .route("/channels", get(list_channels).post(create_channel))
    .route("/channels/{id}", get(get_channel).delete(delete_channel))
    .route("/channels/{id}/nixexprs.tar.xz", get(nixexprs_tarball))
    .route(
      "/channels/{channel_id}/promote/{eval_id}",
      post(promote_channel),
    )
    .route(
      "/projects/{project_id}/channels",
      get(list_project_channels),
    )
}
