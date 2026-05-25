use axum::{
  Router,
  body::Body,
  extract::{Path, State},
  http::StatusCode,
  response::{IntoResponse, Response},
  routing::get,
};
use tokio::process::Command;

use crate::{error::ApiError, state::AppState};

/// Extract the first path info entry from `nix path-info --json` output,
/// handling both the old array format (`[{"path":...}]`) and the new
/// object-keyed format (`{"/nix/store/...": {...}}`).
fn first_path_info_entry(
  parsed: &serde_json::Value,
) -> Option<(&serde_json::Value, Option<&str>)> {
  if let Some(arr) = parsed.as_array() {
    let entry = arr.first()?;
    let path = entry.get("path").and_then(|v| v.as_str());
    Some((entry, path))
  } else if let Some(obj) = parsed.as_object() {
    let (key, val) = obj.iter().next()?;
    Some((val, Some(key.as_str())))
  } else {
    None
  }
}

/// Look up a store path by its nix hash, checking both `build_products` and
/// builds tables.
async fn find_store_path(
  pool: &sqlx::PgPool,
  hash: &str,
) -> std::result::Result<Option<String>, ApiError> {
  let like_pattern = format!("/nix/store/{hash}-%");

  let path: Option<String> = sqlx::query_scalar(
    "SELECT path FROM build_products WHERE path LIKE $1 LIMIT 1",
  )
  .bind(&like_pattern)
  .fetch_optional(pool)
  .await
  .map_err(|e| ApiError(circus_common::CiError::Database(e)))?;

  if path.is_some() {
    return Ok(path);
  }

  sqlx::query_scalar(
    "SELECT build_output_path FROM builds WHERE build_output_path LIKE $1 \
     LIMIT 1",
  )
  .bind(&like_pattern)
  .fetch_optional(pool)
  .await
  .map_err(|e| ApiError(circus_common::CiError::Database(e)))
}

/// Serve `NARInfo` for a store path hash.
/// GET /nix-cache/{hash}.narinfo
async fn narinfo(
  State(state): State<AppState>,
  Path(hash): Path<String>,
) -> Result<Response, ApiError> {
  use std::fmt::Write;

  if !state.config.cache.enabled {
    return Ok(StatusCode::NOT_FOUND.into_response());
  }

  // Strip .narinfo suffix if present
  let hash = hash.strip_suffix(".narinfo").unwrap_or(&hash);

  if !circus_common::validate::is_valid_nix_hash(hash) {
    return Ok(StatusCode::NOT_FOUND.into_response());
  }

  let store_path = match find_store_path(&state.pool, hash).await? {
    Some(p) if circus_common::validate::is_valid_store_path(&p) => p,
    _ => return Ok(StatusCode::NOT_FOUND.into_response()),
  };

  // Get narinfo from nix path-info
  let output = Command::new("nix")
    .args(["path-info", "--json", &store_path])
    .output()
    .await;

  let output = match output {
    Ok(o) if o.status.success() => o,
    _ => return Ok(StatusCode::NOT_FOUND.into_response()),
  };

  let stdout = String::from_utf8_lossy(&output.stdout);
  let parsed: serde_json::Value = match serde_json::from_str(&stdout) {
    Ok(v) => v,
    Err(_) => return Ok(StatusCode::NOT_FOUND.into_response()),
  };

  let Some((entry, path_from_info)) = first_path_info_entry(&parsed) else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };

  let nar_hash = entry.get("narHash").and_then(|v| v.as_str()).unwrap_or("");
  let nar_size = entry
    .get("narSize")
    .and_then(serde_json::Value::as_u64)
    .unwrap_or(0);
  let store_path = path_from_info.unwrap_or(&store_path);

  let refs: Vec<&str> = entry
    .get("references")
    .and_then(|v| v.as_array())
    .map(|arr| {
      arr
        .iter()
        .filter_map(|r| r.as_str())
        .map(|s| s.strip_prefix("/nix/store/").unwrap_or(s))
        .collect()
    })
    .unwrap_or_default();

  // Extract deriver
  let deriver = entry
    .get("deriver")
    .and_then(|v| v.as_str())
    .map(|d| d.strip_prefix("/nix/store/").unwrap_or(d));

  // Extract content-addressable hash
  let ca = entry.get("ca").and_then(|v| v.as_str());

  let file_hash = nar_hash;

  let compression = &state.config.cache.compression;
  let nar_url = format!("nar/{hash}{}", compression.file_extension());
  let compression_str = compression.as_str();

  let refs_joined = refs.join(" ");
  let mut narinfo_text = format!(
    "StorePath: {store_path}\nURL: {nar_url}\nCompression: \
     {compression_str}\nFileHash: {file_hash}\nFileSize: {nar_size}\nNarHash: \
     {nar_hash}\nNarSize: {nar_size}\nReferences: {refs_joined}\n",
  );

  if let Some(deriver) = deriver {
    let _ = writeln!(narinfo_text, "Deriver: {deriver}");
  }
  if let Some(ca) = ca {
    let _ = writeln!(narinfo_text, "CA: {ca}");
  }

  // Optionally sign if secret key is configured
  let narinfo_text =
    if let Some(ref key_file) = state.config.cache.secret_key_file {
      if key_file.exists() {
        sign_narinfo(&narinfo_text, key_file).await
      } else {
        narinfo_text
      }
    } else {
      narinfo_text
    };

  Ok(
    (
      StatusCode::OK,
      [("content-type", "text/x-nix-narinfo")],
      narinfo_text,
    )
      .into_response(),
  )
}

/// Sign narinfo using nix store sign command
async fn sign_narinfo(narinfo: &str, key_file: &std::path::Path) -> String {
  let store_path = narinfo
    .lines()
    .find(|l| l.starts_with("StorePath: "))
    .and_then(|l| l.strip_prefix("StorePath: "));

  let Some(store_path) = store_path else {
    return narinfo.to_string();
  };

  let output = Command::new("nix")
    .args([
      "store",
      "sign",
      "--key-file",
      &key_file.to_string_lossy(),
      store_path,
    ])
    .output()
    .await;

  match output {
    Ok(o) if o.status.success() => {
      let re_output = Command::new("nix")
        .args(["path-info", "--json", store_path])
        .output()
        .await;

      if let Ok(o) = re_output
        && let Ok(parsed) =
          serde_json::from_slice::<serde_json::Value>(&o.stdout)
        && let Some((entry, _)) = first_path_info_entry(&parsed)
        && let Some(sigs) = entry.get("signatures").and_then(|v| v.as_array())
      {
        let sig_lines: Vec<String> = sigs
          .iter()
          .filter_map(|s| s.as_str())
          .map(|s| format!("Sig: {s}"))
          .collect();
        if !sig_lines.is_empty() {
          return format!("{narinfo}{}\n", sig_lines.join("\n"));
        }
      }
      narinfo.to_string()
    },
    _ => narinfo.to_string(),
  }
}

/// Serve a compressed NAR file for a store path.
/// Pipe `nix store dump-path` through an external compressor binary.
/// Both processes are killed on drop so client disconnects are propagated.
fn pipe_through_compressor(
  store_path: &str,
  compressor: &str,
  args: &[&str],
) -> Result<tokio::process::ChildStdout, ApiError> {
  let mut nix_child = std::process::Command::new("nix")
    .args(["store", "dump-path", store_path])
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::null())
    .spawn()
    .map_err(|_| {
      ApiError(circus_common::CiError::Build(
        "Failed to start nix store dump-path".to_string(),
      ))
    })?;

  let nix_stdout = nix_child.stdout.take().ok_or_else(|| {
    ApiError(circus_common::CiError::Build(
      "nix store dump-path produced no stdout".to_string(),
    ))
  })?;

  let mut comp_child = Command::new(compressor)
    .args(args)
    .stdin(nix_stdout)
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::null())
    .kill_on_drop(true)
    .spawn()
    .map_err(|_| {
      ApiError(circus_common::CiError::Build(format!(
        "Failed to start {compressor}"
      )))
    })?;

  comp_child.stdout.take().ok_or_else(|| {
    ApiError(circus_common::CiError::Build(format!(
      "{compressor} produced no stdout"
    )))
  })
}

/// Serve a NAR file with the requested compression algorithm.
/// Routes for all `.nar`, `.nar.zst`, `.nar.bz2`, `.nar.xz` suffixes funnel
/// here.
async fn serve_nar_combined(
  State(state): State<AppState>,
  Path(hash): Path<String>,
) -> Result<Response, ApiError> {
  if !state.config.cache.enabled {
    return Ok(StatusCode::NOT_FOUND.into_response());
  }

  let (stripped, content_type, compressor): (
    &str,
    &'static str,
    Option<(&'static str, &'static [&'static str])>,
  ) = if let Some(s) = hash.strip_suffix(".nar.zst") {
    (s, "application/zstd", Some(("zstd", &["-c"])))
  } else if let Some(s) = hash.strip_suffix(".nar.bz2") {
    (s, "application/x-bzip2", Some(("bzip2", &["-c"])))
  } else if let Some(s) = hash.strip_suffix(".nar.xz") {
    (s, "application/x-xz", Some(("xz", &["-c"])))
  } else if let Some(s) = hash.strip_suffix(".nar") {
    (s, "application/x-nix-nar", None)
  } else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };

  if !circus_common::validate::is_valid_nix_hash(stripped) {
    return Ok(StatusCode::NOT_FOUND.into_response());
  }

  let store_path = match find_store_path(&state.pool, stripped).await? {
    Some(p) if circus_common::validate::is_valid_store_path(&p) => p,
    _ => return Ok(StatusCode::NOT_FOUND.into_response()),
  };

  let body = if let Some((bin, args)) = compressor {
    let stdout = pipe_through_compressor(&store_path, bin, args)?;
    Body::from_stream(tokio_util::io::ReaderStream::new(stdout))
  } else {
    let mut child = Command::new("nix")
      .args(["store", "dump-path", &store_path])
      .stdout(std::process::Stdio::piped())
      .stderr(std::process::Stdio::null())
      .kill_on_drop(true)
      .spawn()
      .map_err(|_| {
        ApiError(circus_common::CiError::Build(
          "Failed to start nix store dump-path".to_string(),
        ))
      })?;
    let stdout = child.stdout.take().ok_or_else(|| {
      ApiError(circus_common::CiError::Build(
        "nix store dump-path produced no stdout".to_string(),
      ))
    })?;
    Body::from_stream(tokio_util::io::ReaderStream::new(stdout))
  };

  Ok((StatusCode::OK, [("content-type", content_type)], body).into_response())
}

/// Nix binary cache info endpoint.
/// GET /nix-cache/nix-cache-info
async fn cache_info(State(state): State<AppState>) -> Response {
  if !state.config.cache.enabled {
    return StatusCode::NOT_FOUND.into_response();
  }

  let info = "StoreDir: /nix/store\nWantMassQuery: 1\nPriority: 30\n";

  (StatusCode::OK, [("content-type", "text/plain")], info).into_response()
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/nix-cache/nix-cache-info", get(cache_info))
    .route("/nix-cache/{hash}", get(narinfo))
    .route("/nix-cache/nar/{hash}", get(serve_nar_combined))
}
