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

/// Escape a string for use as a Nix string literal.
fn nix_escape_string(s: &str) -> String {
  let mut out = String::with_capacity(s.len() + 2);
  out.push('"');
  for ch in s.chars() {
    match ch {
      '"' => out.push_str("\\\""),
      '\\' => out.push_str("\\\\"),
      '\n' => out.push_str("\\n"),
      '\r' => out.push_str("\\r"),
      '\t' => out.push_str("\\t"),
      '$' => out.push_str("\\$"),
      c => out.push(c),
    }
  }
  out.push('"');
  out
}

/// True if `ident` is a Nix attribute name that can appear unquoted.
/// Matches `[A-Za-z_][A-Za-z0-9_'-]*`.
fn is_bare_nix_ident(ident: &str) -> bool {
  let mut bytes = ident.bytes();
  let Some(first) = bytes.next() else {
    return false;
  };
  if !(first.is_ascii_alphabetic() || first == b'_') {
    return false;
  }
  bytes
    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'\'' || b == b'-')
}

/// Render a dotted attribute path as a Nix attr path, quoting each segment
/// that isn't a bare identifier. Empty segments are dropped, which matches
/// Hydra's behaviour for jobs like `.hello` or `foo..bar`.
fn render_attr_path(job_name: &str) -> String {
  job_name
    .split('.')
    .filter(|s| !s.is_empty())
    .map(|seg| {
      if is_bare_nix_ident(seg) {
        seg.to_string()
      } else {
        nix_escape_string(seg)
      }
    })
    .collect::<Vec<_>>()
    .join(".")
}

/// Collect every non-leaf prefix of an attribute path. Given `foo.bar.baz`
/// returns `["foo", "foo.bar"]`, each rendered to its Nix attr-path form.
fn intermediate_paths(job_name: &str) -> Vec<String> {
  let segs: Vec<&str> = job_name.split('.').filter(|s| !s.is_empty()).collect();
  if segs.len() < 2 {
    return Vec::new();
  }
  (1..segs.len())
    .map(|i| {
      segs[..i]
        .iter()
        .map(|seg| {
          if is_bare_nix_ident(seg) {
            (*seg).to_string()
          } else {
            nix_escape_string(seg)
          }
        })
        .collect::<Vec<_>>()
        .join(".")
    })
    .collect()
}

/// Resolve a build's output map. Prefers the normalized `build_outputs`
/// table; if it's empty (older rows or aggregates) falls back to a
/// synthetic `{"out": build_output_path}`.
async fn collect_outputs(
  pool: &sqlx::PgPool,
  build: &circus_common::models::Build,
) -> std::collections::BTreeMap<String, String> {
  use std::collections::BTreeMap;

  let mut outputs: BTreeMap<String, String> = BTreeMap::new();

  if let Ok(rows) =
    circus_common::repo::build_outputs::list_for_build(pool, build.id).await
  {
    for row in rows {
      if let Some(path) = row.path {
        outputs.insert(row.name, path);
      }
    }
  }

  if outputs.is_empty()
    && let Some(path) = build.build_output_path.clone()
  {
    outputs.insert("out".to_string(), path);
  }

  outputs
}

/// Build the `nixexprs.tar.xz` payload for an evaluation, matching the
/// Hydra channel layout that `nix-channel --update` (and consumers
/// pinning by sha256) expect:
///
/// * archive contains a single top-level directory named `channel/`,
/// * `channel/channel-name` carries the channel's display name as plain text,
/// * `channel/default.nix` exposes each succeeded build as a fake derivation,
///   branched per `system`, with multi-output and dotted attribute path
///   support.
///
/// The tar headers use a fixed mtime/uid/gid/owner so the bytes are
/// deterministic for a given (channel name, evaluation) input. Clients
/// can pin the resulting tarball by sha256.
///
/// # Errors
///
/// Returns `NotFound` when the evaluation has no succeeded builds, or
/// a `Build` error if archive construction fails.
pub async fn build_nixexprs_tarball(
  pool: &sqlx::PgPool,
  channel_name: &str,
  evaluation_id: Uuid,
) -> Result<Vec<u8>, ApiError> {
  use std::collections::{BTreeMap, BTreeSet};

  let builds =
    circus_common::repo::builds::list_for_evaluation(pool, evaluation_id)
      .await
      .map_err(ApiError)?;

  // Group by system. Skip builds with no system attribute or no outputs;
  // they can't be expressed as a per-system fake derivation.
  let mut by_system: BTreeMap<String, Vec<&circus_common::models::Build>> =
    BTreeMap::new();
  for build in &builds {
    if build.status != BuildStatus::Succeeded {
      continue;
    }
    let Some(system) = build.system.clone() else {
      continue;
    };
    by_system.entry(system).or_default().push(build);
  }

  if by_system.is_empty() {
    return Err(ApiError(circus_common::CiError::NotFound(
      "No succeeded builds with a system attribute in current evaluation"
        .to_string(),
    )));
  }

  // Sort each system's builds by (job_name, id) so repeated requests
  // produce the same bytes.
  for builds in by_system.values_mut() {
    builds.sort_by(|a, b| a.job_name.cmp(&b.job_name).then(a.id.cmp(&b.id)));
  }

  let mut nix_src = String::with_capacity(1024 + builds.len() * 240);
  nix_src.push_str(
    "{ system ? builtins.currentSystem }:\n\nlet\n\n  maybeStorePath = if \
     builtins ? langVersion && builtins.lessThan 1 builtins.langVersion\n    \
     then builtins.storePath\n    else x: x;\n\n  mkFakeDerivation = attrs: \
     outputs:\n    let\n      outputNames = builtins.attrNames outputs;\n      \
     common = attrs // outputsSet //\n        { type = \"derivation\";\n          \
     outputs = outputNames;\n          all = outputsList;\n        };\n      \
     outputToAttrListElement = outputName:\n        { name = outputName;\n          \
     value = common // {\n            inherit outputName;\n            outPath = \
     maybeStorePath (builtins.getAttr outputName outputs);\n          };\n        \
     };\n      outputsList = map outputToAttrListElement outputNames;\n      \
     outputsSet = builtins.listToAttrs outputsList;\n    in outputsSet;\n\nin\n\n",
  );

  let mut first = true;
  for (system, system_builds) in &by_system {
    if !first {
      nix_src.push_str("else ");
    }
    let _ = writeln!(
      nix_src,
      "if system == {} then {{\n",
      nix_escape_string(system)
    );

    let mut intermediates: BTreeSet<String> = BTreeSet::new();

    for build in system_builds {
      let outputs = collect_outputs(pool, build).await;
      if outputs.is_empty() {
        continue;
      }
      let attr_path = render_attr_path(&build.job_name);
      if attr_path.is_empty() {
        continue;
      }
      for p in intermediate_paths(&build.job_name) {
        intermediates.insert(p);
      }

      let _ = writeln!(nix_src, "  # Circus build {}", build.id);
      let _ = writeln!(nix_src, "  {attr_path} = (mkFakeDerivation {{");
      let _ = writeln!(nix_src, "    type = \"derivation\";");
      let _ = writeln!(
        nix_src,
        "    name = {};",
        nix_escape_string(&build.job_name)
      );
      let _ = writeln!(nix_src, "    system = {};", nix_escape_string(system));

      let has_meta = build.meta_description.is_some()
        || build.meta_license.is_some()
        || build.meta_homepage.is_some()
        || build.meta_maintainers.is_some();
      if has_meta {
        nix_src.push_str("    meta = {\n");
        if let Some(d) = &build.meta_description {
          let _ =
            writeln!(nix_src, "      description = {};", nix_escape_string(d));
        }
        if let Some(l) = &build.meta_license {
          let _ =
            writeln!(nix_src, "      license = {};", nix_escape_string(l));
        }
        if let Some(h) = &build.meta_homepage {
          let _ =
            writeln!(nix_src, "      homepage = {};", nix_escape_string(h));
        }
        if let Some(m) = &build.meta_maintainers {
          let _ =
            writeln!(nix_src, "      maintainers = {};", nix_escape_string(m));
        }
        nix_src.push_str("    };\n");
      }

      nix_src.push_str("  } {\n");
      for (name, path) in &outputs {
        let _ = writeln!(
          nix_src,
          "    {} = {};",
          nix_escape_string(name),
          nix_escape_string(path)
        );
      }
      let default_output = if outputs.contains_key("out") {
        "out"
      } else {
        outputs.keys().next().map(String::as_str).unwrap_or("out")
      };
      let _ = writeln!(
        nix_src,
        "  }}).{};\n",
        if is_bare_nix_ident(default_output) {
          default_output.to_string()
        } else {
          nix_escape_string(default_output)
        }
      );
    }

    for p in &intermediates {
      let _ = writeln!(nix_src, "  {p}.recurseForDerivations = true;\n");
    }

    nix_src.push_str("}\n\n");
    first = false;
  }

  if !first {
    nix_src.push_str("else ");
  }
  nix_src.push_str("{}\n");

  let channel_name = channel_name.to_string();
  // Build the deterministic tar in a blocking task (tar::Builder is sync),
  // then xz-encode the resulting bytes through async-compression. This
  // keeps the project on one lzma backend.
  let tar_bytes =
    tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
      let mut buf = Vec::new();
      {
        let mut tar_builder = tar::Builder::new(&mut buf);
        append_deterministic(
          &mut tar_builder,
          "channel/channel-name",
          channel_name.as_bytes(),
        )?;
        append_deterministic(
          &mut tar_builder,
          "channel/default.nix",
          nix_src.as_bytes(),
        )?;
        tar_builder
          .into_inner()
          .map_err(|e| format!("Failed to finish tar: {e}"))?;
      }
      Ok(buf)
    })
    .await
    .map_err(|e| {
      ApiError(circus_common::CiError::Build(format!(
        "Task join error: {e}"
      )))
    })?
    .map_err(|e| ApiError(circus_common::CiError::Build(e)))?;

  encode_xz_async(tar_bytes).await.map_err(|e| {
    ApiError(circus_common::CiError::Build(format!(
      "xz encode failed: {e}"
    )))
  })
}

/// Encode `bytes` with xz (level 6) via async-compression. Identical
/// shape to the helper in `channel_manifests.rs`; kept private here to
/// avoid a route-module cross-import.
async fn encode_xz_async(bytes: Vec<u8>) -> std::io::Result<Vec<u8>> {
  use async_compression::tokio::write::XzEncoder;
  use tokio::io::AsyncWriteExt as _;
  let mut enc = XzEncoder::with_quality(
    Vec::with_capacity(bytes.len()),
    async_compression::Level::Precise(6),
  );
  enc.write_all(&bytes).await?;
  enc.shutdown().await?;
  Ok(enc.into_inner())
}

/// Append a single file to a tar archive with fixed metadata (mtime=1,
/// uid=gid=0, owner/group empty). Matches Hydra's `mtime => 1` choice so
/// the produced bytes are deterministic across requests.
fn append_deterministic<W: std::io::Write>(
  tar_builder: &mut tar::Builder<W>,
  path: &str,
  data: &[u8],
) -> Result<(), String> {
  let mut header = tar::Header::new_gnu();
  header.set_size(data.len() as u64);
  header.set_mode(0o644);
  header.set_mtime(1);
  header.set_uid(0);
  header.set_gid(0);
  header.set_entry_type(tar::EntryType::Regular);
  if let Err(e) = header.set_username("") {
    return Err(format!("Failed to set tar username: {e}"));
  }
  if let Err(e) = header.set_groupname("") {
    return Err(format!("Failed to set tar groupname: {e}"));
  }
  header.set_cksum();
  tar_builder
    .append_data(&mut header, path, data)
    .map_err(|e| format!("Failed to append {path}: {e}"))
}

/// Generate and serve `nixexprs.tar.xz` for Nix channel compatibility.
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

  let xz_data =
    build_nixexprs_tarball(&state.pool, &channel.name, evaluation_id).await?;

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

/// Channel management routes: CRUD for release channels, Nix channel tarball
/// serving, and manual evaluation promotion.
///
/// # Returns
///
/// A router with the following routes mounted:
///
/// - `GET /channels` - list all channels
/// - `POST /channels` - create a channel (admin only)
/// - `GET /channels/{id}` - get a channel by ID
/// - `DELETE /channels/{id}` - delete a channel (admin only)
/// - `GET /channels/{id}/nixexprs.tar.xz` - serve the Nix channel tarball
/// - `POST /channels/{channel_id}/promote/{eval_id}` - promote an evaluation to
///   a channel (admin only)
/// - `GET /projects/{project_id}/channels` - list channels for a project
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn nix_escape_basic() {
    assert_eq!(nix_escape_string("hello"), r#""hello""#);
    assert_eq!(nix_escape_string("with \"quote\""), r#""with \"quote\"""#);
    assert_eq!(nix_escape_string("a\\b"), r#""a\\b""#);
    assert_eq!(nix_escape_string("$var"), r#""\$var""#);
    assert_eq!(nix_escape_string("line\n"), r#""line\n""#);
  }

  #[test]
  fn bare_ident_classifier() {
    assert!(is_bare_nix_ident("foo"));
    assert!(is_bare_nix_ident("_x"));
    assert!(is_bare_nix_ident("a-b"));
    assert!(is_bare_nix_ident("a'"));
    assert!(!is_bare_nix_ident(""));
    assert!(!is_bare_nix_ident("1foo"));
    assert!(!is_bare_nix_ident("a.b"));
    assert!(!is_bare_nix_ident("with space"));
  }

  #[test]
  fn attr_path_quotes_segments_that_need_it() {
    assert_eq!(render_attr_path("hello"), "hello");
    assert_eq!(render_attr_path("foo.bar.baz"), "foo.bar.baz");
    assert_eq!(
      render_attr_path("nixpkgs.python3Packages.requests"),
      "nixpkgs.python3Packages.requests"
    );
    assert_eq!(
      render_attr_path("checks.x86_64-linux.full"),
      "checks.x86_64-linux.full"
    );
    assert_eq!(render_attr_path("foo.1bar.baz"), r#"foo."1bar".baz"#);
  }

  #[test]
  fn intermediate_paths_for_recurse_markers() {
    assert!(intermediate_paths("hello").is_empty());
    assert_eq!(intermediate_paths("a.b"), vec!["a".to_string()]);
    assert_eq!(intermediate_paths("a.b.c"), vec![
      "a".to_string(),
      "a.b".to_string()
    ]);
  }
}
