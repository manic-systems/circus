use std::{
  pin::Pin,
  task::{Context, Poll},
  time::Duration,
};

use axum::{
  Router,
  body::Body,
  extract::{Path, State},
  http::{HeaderValue, StatusCode, header},
  response::{IntoResponse, Response},
  routing::get,
};
use tokio::{
  io::{AsyncRead, ReadBuf},
  process::{Child, ChildStdout, Command},
};

use crate::{error::ApiError, state::AppState};

const S3_GET_PRESIGN_EXPIRY: Duration = Duration::from_hours(1);
const MAX_NAR_OBJECT_NAME_LEN: usize = 512;

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

/// Look up a store path by its nix hash, limited to build outputs that Circus
/// signed at build time. This intentionally does not fall back to arbitrary
/// `/nix/store` paths.
async fn find_signed_store_path(
  pool: &sqlx::PgPool,
  hash: &str,
  store_dir: &str,
) -> std::result::Result<Option<String>, ApiError> {
  let store_dir = store_dir.trim_end_matches('/');
  let like_pattern = format!("{store_dir}/{hash}-%");

  let path: Option<String> = sqlx::query_scalar(
    "SELECT bp.path FROM build_products bp JOIN builds b ON b.id = \
     bp.build_id WHERE bp.path LIKE $1 AND b.signed = true LIMIT 1",
  )
  .bind(&like_pattern)
  .fetch_optional(pool)
  .await
  .map_err(|e| ApiError(circus_common::CiError::Database(e)))?;

  if path.is_some() {
    return Ok(path);
  }

  let from_builds = sqlx::query_scalar(
    "SELECT build_output_path FROM builds WHERE build_output_path LIKE $1 AND \
     signed = true LIMIT 1",
  )
  .bind(&like_pattern)
  .fetch_optional(pool)
  .await
  .map_err(|e| ApiError(circus_common::CiError::Database(e)))?;

  Ok(from_builds)
}

fn narinfo_has_signature(
  row: &circus_common::repo::narinfo_cache::NarInfo,
) -> bool {
  row.sig.as_ref().is_some_and(|sig| !sig.trim().is_empty())
}

/// Resolve the local store path a cache request may serve. Signed build
/// outputs come from the DB; any other local path is only servable when
/// content-addressed (`true` in the second tuple field), which covers the
/// drv closures (drvs and sources) agents substitute for dispatched
/// builds. Nix recomputes the store path from the `CA:` field on
/// substitution, so CA paths cannot be forged and need no signature.
///
/// FIXME: this shells out to `nix`, and needs the nix-command feature. We
/// should bind to the Nix C/C++ API and call it directly instead.
async fn resolve_servable_path(
  pool: &sqlx::PgPool,
  hash: &str,
  store_dir: &str,
) -> std::result::Result<Option<(String, bool)>, ApiError> {
  if let Some(p) = find_signed_store_path(pool, hash, store_dir).await? {
    return Ok(
      circus_common::validate::is_valid_store_path(&p, store_dir)
        .then_some((p, false)),
    );
  }

  let resolved = Command::new("nix")
    .args(["store", "path-from-hash-part", hash])
    .output()
    .await
    .ok()
    .filter(|o| o.status.success())
    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    .filter(|p| circus_common::validate::is_valid_store_path(p, store_dir));

  Ok(resolved.map(|p| (p, true)))
}

/// Whether the local store path is content-addressed (drvs, sources).
async fn is_content_addressed(store_path: &str) -> bool {
  let output = Command::new("nix")
    .args(["path-info", "--json", store_path])
    .output()
    .await;
  let Ok(output) = output else {
    return false;
  };
  if !output.status.success() {
    return false;
  }
  let Ok(parsed) = serde_json::from_slice::<serde_json::Value>(&output.stdout)
  else {
    return false;
  };
  first_path_info_entry(&parsed)
    .and_then(|(entry, _)| entry.get("ca"))
    .and_then(|v| v.as_str())
    .is_some_and(|ca| !ca.is_empty())
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

  if let Some(cached) = state.narinfo_cache.get(hash) {
    return Ok(
      (
        StatusCode::OK,
        [("content-type", "text/x-nix-narinfo")],
        cached,
      )
        .into_response(),
    );
  }

  // Persistent narinfo from the agents' presigned upload flow. This
  // table sees every successful upload across the cluster, so a path
  // built on one builder is available from any cache fetcher without
  // running nix path-info locally.
  if let Ok(row) =
    circus_common::repo::narinfo_cache::get_by_hash_part(&state.pool, hash)
      .await
    && narinfo_has_signature(&row)
  {
    let body = render_narinfo_row(&row);
    state.narinfo_cache.insert(hash.to_owned(), body.clone());
    return Ok(
      (
        StatusCode::OK,
        [("content-type", "text/x-nix-narinfo")],
        body,
      )
        .into_response(),
    );
  }

  let store_dir = state.config.nix.store_dir.to_string_lossy();
  let store_dir = store_dir.trim_end_matches('/');
  let Some((store_path, require_ca)) =
    resolve_servable_path(&state.pool, hash, store_dir).await?
  else {
    return Ok(StatusCode::NOT_FOUND.into_response());
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

  let store_prefix = format!("{store_dir}/");
  let refs: Vec<&str> = entry
    .get("references")
    .and_then(|v| v.as_array())
    .map(|arr| {
      arr
        .iter()
        .filter_map(|r| r.as_str())
        .map(|s| s.strip_prefix(store_prefix.as_str()).unwrap_or(s))
        .collect()
    })
    .unwrap_or_default();

  // Extract deriver
  let deriver = entry
    .get("deriver")
    .and_then(|v| v.as_str())
    .map(|d| d.strip_prefix(store_prefix.as_str()).unwrap_or(d));

  // Extract content-addressable hash
  let ca = entry.get("ca").and_then(|v| v.as_str());

  let signatures: Vec<&str> = entry
    .get("signatures")
    .and_then(|v| v.as_array())
    .map(|arr| arr.iter().filter_map(|s| s.as_str()).collect())
    .unwrap_or_default();
  // Outputs must carry a signature minted at build time; drv-closure
  // paths are only trusted through their CA field.
  let servable = if require_ca {
    ca.is_some_and(|c| !c.is_empty())
  } else {
    !signatures.is_empty()
  };
  if !servable {
    return Ok(StatusCode::NOT_FOUND.into_response());
  }

  let compression = &state.config.cache.compression;
  let nar_url = format!("nar/{hash}{}", compression.file_extension());
  let compression_str = compression.as_str();
  let is_uncompressed =
    matches!(compression, circus_common::config::NarCompression::None);

  let refs_joined = refs.join(" ");
  // FileHash / FileSize describe the compressed file being served. We can
  // only set them honestly when compression is none (then they equal the
  // NAR hash/size). For compressed responses we'd have to buffer the full
  // compressed stream to hash it; omit the fields instead. Nix clients
  // fall back to validating NarHash after decompression.
  let mut narinfo_text = if is_uncompressed {
    format!(
      "StorePath: {store_path}\nURL: {nar_url}\nCompression: \
       {compression_str}\nFileHash: {nar_hash}\nFileSize: \
       {nar_size}\nNarHash: {nar_hash}\nNarSize: {nar_size}\nReferences: \
       {refs_joined}\n",
    )
  } else {
    format!(
      "StorePath: {store_path}\nURL: {nar_url}\nCompression: \
       {compression_str}\nNarHash: {nar_hash}\nNarSize: \
       {nar_size}\nReferences: {refs_joined}\n",
    )
  };

  if let Some(deriver) = deriver {
    let _ = writeln!(narinfo_text, "Deriver: {deriver}");
  }
  if let Some(ca) = ca {
    let _ = writeln!(narinfo_text, "CA: {ca}");
  }
  for sig in signatures {
    let _ = writeln!(narinfo_text, "Sig: {sig}");
  }

  state
    .narinfo_cache
    .insert(hash.to_string(), narinfo_text.clone());

  Ok(
    (
      StatusCode::OK,
      [("content-type", "text/x-nix-narinfo")],
      narinfo_text,
    )
      .into_response(),
  )
}

/// Render a `narinfo_cache::NarInfo` row to the on-the-wire narinfo
/// format. Mirrors the field order emitted by the path-info path so a
/// substituter can't tell the two sources apart.
fn render_narinfo_row(
  row: &circus_common::repo::narinfo_cache::NarInfo,
) -> String {
  use std::fmt::Write as _;
  let mut s = String::new();
  let _ = writeln!(s, "StorePath: {}", row.store_path);
  let _ = writeln!(s, "URL: {}", row.url);
  let _ = writeln!(s, "Compression: {}", row.compression);
  if let Some(fh) = &row.file_hash {
    let _ = writeln!(s, "FileHash: {fh}");
  }
  if let Some(fs) = row.file_size {
    let _ = writeln!(s, "FileSize: {fs}");
  }
  let _ = writeln!(s, "NarHash: {}", row.nar_hash);
  let _ = writeln!(s, "NarSize: {}", row.nar_size);
  let _ = writeln!(s, "References: {}", row.references.join(" "));
  if let Some(d) = &row.deriver {
    let _ = writeln!(s, "Deriver: {d}");
  }
  if let Some(c) = &row.ca {
    let _ = writeln!(s, "CA: {c}");
  }
  if let Some(sig) = &row.sig {
    let _ = writeln!(s, "Sig: {sig}");
  }
  s
}

fn uploaded_nar_presigner(
  config: &circus_common::config::Config,
) -> Option<circus_common::s3::Presigner> {
  let uri = config.cache_upload.store_uri.as_deref()?;
  let s3 = config.cache_upload.s3.as_ref()?;
  circus_common::s3::Presigner::from_config(uri, s3)
}

fn is_valid_nar_object_name(name: &str) -> bool {
  !name.is_empty()
    && name.len() <= MAX_NAR_OBJECT_NAME_LEN
    && name.contains(".nar")
    && name.bytes().all(|b| {
      matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.')
    })
}

async fn redirect_uploaded_nar(
  state: &AppState,
  object_name: &str,
) -> Result<Option<Response>, ApiError> {
  if !is_valid_nar_object_name(object_name) {
    return Ok(None);
  }

  let url = format!("nar/{object_name}");
  match circus_common::repo::narinfo_cache::get_by_url(&state.pool, &url).await
  {
    Ok(row) => {
      if !narinfo_has_signature(&row) {
        return Ok(Some(StatusCode::NOT_FOUND.into_response()));
      }
      let Some(presigner) = uploaded_nar_presigner(&state.config) else {
        tracing::warn!(
          url = %row.url,
          "uploaded NAR exists but cache_upload S3 presigning is not configured"
        );
        return Ok(Some(StatusCode::NOT_FOUND.into_response()));
      };

      let signed_url = presigner.presign_get(&row.url, S3_GET_PRESIGN_EXPIRY);
      let Ok(location) = HeaderValue::from_str(&signed_url) else {
        tracing::warn!(url = %row.url, "failed to construct S3 redirect URL");
        return Ok(Some(StatusCode::NOT_FOUND.into_response()));
      };
      let mut response = StatusCode::TEMPORARY_REDIRECT.into_response();
      response.headers_mut().insert(header::LOCATION, location);
      Ok(Some(response))
    },
    Err(circus_common::CiError::NotFound(_)) => Ok(None),
    Err(e) => Err(ApiError(e)),
  }
}

/// Serve a compressed NAR file for a store path.
/// Pipe `nix store dump-path` through an external compressor binary.
/// Both processes are killed on drop so client disconnects are propagated.
struct ChildOutput {
  children: Vec<OwnedChild>,
  stdout:   ChildStdout,
}

enum OwnedChild {
  Std(std::process::Child),
  Tokio(Child),
}

impl OwnedChild {
  fn kill(&mut self) -> std::io::Result<()> {
    match self {
      Self::Std(child) => child.kill(),
      Self::Tokio(child) => child.start_kill(),
    }
  }
}

impl AsyncRead for ChildOutput {
  fn poll_read(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
  ) -> Poll<std::io::Result<()>> {
    Pin::new(&mut self.stdout).poll_read(cx, buf)
  }
}

impl Drop for ChildOutput {
  fn drop(&mut self) {
    for child in &mut self.children {
      if let Err(e) = child.kill() {
        tracing::debug!("Failed to kill cache stream child process: {e}");
      }
    }
  }
}

fn pipe_through_compressor(
  store_path: &str,
  compressor: &str,
  args: &[&str],
) -> Result<ChildOutput, ApiError> {
  // Inherit stderr so a failing nix/compressor child shows up in the
  // server log instead of truncating the response body silently.
  let mut nix_child = std::process::Command::new("nix")
    .args(["store", "dump-path", store_path])
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::inherit())
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
    .stderr(std::process::Stdio::inherit())
    .kill_on_drop(true)
    .spawn()
    .map_err(|_| {
      ApiError(circus_common::CiError::Build(format!(
        "Failed to start {compressor}"
      )))
    })?;

  let stdout = comp_child.stdout.take().ok_or_else(|| {
    ApiError(circus_common::CiError::Build(format!(
      "{compressor} produced no stdout"
    )))
  })?;

  Ok(ChildOutput {
    children: vec![OwnedChild::Std(nix_child), OwnedChild::Tokio(comp_child)],
    stdout,
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
  } else if let Some(s) = hash.strip_suffix(".nar.gz") {
    (s, "application/gzip", Some(("gzip", &["-c"])))
  } else if let Some(s) = hash.strip_suffix(".nar.bz2") {
    (s, "application/x-bzip2", Some(("bzip2", &["-c"])))
  } else if let Some(s) = hash.strip_suffix(".nar.br") {
    (s, "application/brotli", Some(("brotli", &["-c"])))
  } else if let Some(s) = hash.strip_suffix(".nar.xz") {
    (s, "application/x-xz", Some(("xz", &["-c"])))
  } else if let Some(s) = hash.strip_suffix(".nar") {
    (s, "application/x-nix-nar", None)
  } else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };

  if let Some(response) = redirect_uploaded_nar(&state, &hash).await? {
    return Ok(response);
  }

  if !circus_common::validate::is_valid_nix_hash(stripped) {
    return Ok(StatusCode::NOT_FOUND.into_response());
  }

  let store_dir = state.config.nix.store_dir.to_string_lossy();
  let store_dir = store_dir.trim_end_matches('/');
  let Some((store_path, require_ca)) =
    resolve_servable_path(&state.pool, stripped, store_dir).await?
  else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };
  if require_ca && !is_content_addressed(&store_path).await {
    return Ok(StatusCode::NOT_FOUND.into_response());
  }

  let body = if let Some((bin, args)) = compressor {
    let stdout = pipe_through_compressor(&store_path, bin, args)?;
    Body::from_stream(tokio_util::io::ReaderStream::new(stdout))
  } else {
    let mut child = Command::new("nix")
      .args(["store", "dump-path", &store_path])
      .stdout(std::process::Stdio::piped())
      .stderr(std::process::Stdio::inherit())
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
    Body::from_stream(tokio_util::io::ReaderStream::new(ChildOutput {
      children: vec![OwnedChild::Tokio(child)],
      stdout,
    }))
  };

  Ok((StatusCode::OK, [("content-type", content_type)], body).into_response())
}

/// Nix binary cache info endpoint.
/// GET /nix-cache/nix-cache-info
async fn cache_info(State(state): State<AppState>) -> Response {
  if !state.config.cache.enabled {
    return StatusCode::NOT_FOUND.into_response();
  }

  let store_dir = state.config.nix.store_dir.display();
  let info = format!("StoreDir: {store_dir}\nWantMassQuery: 1\nPriority: 30\n");

  (StatusCode::OK, [("content-type", "text/plain")], info).into_response()
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/nix-cache/nix-cache-info", get(cache_info))
    .route("/nix-cache/{hash}", get(narinfo))
    .route("/nix-cache/nar/{hash}", get(serve_nar_combined))
}
