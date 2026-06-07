use std::{collections::BTreeSet, num::NonZero, path::PathBuf, time::Duration};

use axum::{
  Router,
  body::Body,
  extract::{Path, Query, State},
  http::{HeaderValue, StatusCode, header},
  response::{IntoResponse, Response},
  routing::get,
};
use harmonia_file_nar::NarByteStream;
use harmonia_store_content_address::ContentAddress;
use harmonia_store_nar_info::{build_narinfo, format_narinfo_txt};
use harmonia_store_path::{StoreDir, StorePath, StorePathHash};
use harmonia_store_path_info::{UnkeyedValidPathInfo, ValidPathInfo};
use harmonia_utils_hash::{Hash, HashFormat as _, fmt::Any as AnyHashFmt};
use serde::Deserialize;
use sqlx::{FromRow, PgPool, SqlitePool};

use crate::{error::ApiError, state::AppState};

const S3_GET_PRESIGN_EXPIRY: Duration = Duration::from_hours(1);
const MAX_NAR_OBJECT_NAME_LEN: usize = 512;

#[derive(FromRow)]
struct ValidPathRow {
  id:                i64,
  path:              String,
  #[sqlx(rename = "hash")]
  nar_hash:          String,
  #[sqlx(rename = "registrationTime")]
  registration_time: i64,
  deriver:           Option<String>,
  #[sqlx(rename = "narSize")]
  nar_size:          Option<i64>,
  ultimate:          Option<i32>,
  sigs:              Option<String>,
  ca:                Option<String>,
}

#[derive(Deserialize)]
struct NarQuery {
  hash: Option<String>,
}

fn cache_data_error(error: impl std::fmt::Display) -> ApiError {
  ApiError(circus_common::CiError::Internal(format!(
    "invalid Nix store cache data: {error}"
  )))
}

async fn open_nix_store_db(state: &AppState) -> Option<&SqlitePool> {
  match state.nix_store.open_db().await {
    Ok(db) => db,
    Err(e) => {
      tracing::warn!("failed to open local Nix store DB for binary cache: {e}");
      None
    },
  }
}

fn narinfo_has_signature(
  row: &circus_common::repo::narinfo_cache::NarInfo,
) -> bool {
  row.sig.as_ref().is_some_and(|sig| !sig.trim().is_empty())
}

async fn query_harmonia_path_info(
  hash: &str,
  store_dir: &StoreDir,
  nix_store_db: &SqlitePool,
) -> Result<Option<ValidPathInfo>, ApiError> {
  let Ok(hash) = StorePathHash::decode_digest(hash.as_bytes()) else {
    return Ok(None);
  };
  let prefix = format!("{store_dir}/{hash}");

  let Some(row) = sqlx::query_as::<_, ValidPathRow>(
    "SELECT id, path, hash, registrationTime, deriver, narSize, ultimate, \
     sigs, ca FROM ValidPaths WHERE path >= ?1 ORDER BY path ASC LIMIT 1",
  )
  .bind(&prefix)
  .fetch_optional(nix_store_db)
  .await
  .map_err(|e| ApiError(circus_common::CiError::Database(e)))?
  else {
    return Ok(None);
  };

  if !row.path.starts_with(&prefix) {
    return Ok(None);
  }

  let references = sqlx::query_scalar::<_, String>(
    "SELECT v.path FROM Refs r JOIN ValidPaths v ON r.reference = v.id WHERE \
     r.referrer = ?1",
  )
  .bind(row.id)
  .fetch_all(nix_store_db)
  .await
  .map_err(|e| ApiError(circus_common::CiError::Database(e)))?
  .into_iter()
  .filter_map(|path| store_dir.parse(&path).ok())
  .collect::<BTreeSet<_>>();

  let Ok(path) = store_dir.parse::<StorePath>(&row.path) else {
    return Ok(None);
  };
  let deriver = row
    .deriver
    .and_then(|path| store_dir.parse::<StorePath>(&path).ok());
  let Ok(nar_hash) = row
    .nar_hash
    .parse::<AnyHashFmt<harmonia_store_path_info::NarHash>>()
  else {
    return Ok(None);
  };
  let nar_hash = nar_hash.into_hash();
  let signatures = row
    .sigs
    .as_deref()
    .map(|sigs| {
      sigs
        .split_whitespace()
        .filter_map(|sig| sig.parse().ok())
        .collect()
    })
    .unwrap_or_default();
  let ca = row.ca.and_then(|ca| ca.parse::<ContentAddress>().ok());

  Ok(Some(ValidPathInfo {
    path,
    info: UnkeyedValidPathInfo {
      deriver,
      nar_hash,
      references,
      registration_time: NonZero::new(row.registration_time),
      nar_size: row.nar_size.map_or(0, |n| n as u64),
      ultimate: row.ultimate.unwrap_or(0) != 0,
      signatures,
      ca,
      store_dir: store_dir.clone(),
    },
  }))
}

async fn has_circus_signed_build_product(
  pool: &PgPool,
  store_path: &str,
) -> Result<bool, ApiError> {
  sqlx::query_scalar::<_, bool>(
    "SELECT EXISTS(SELECT 1 FROM build_products bp JOIN builds b ON b.id = \
     bp.build_id WHERE bp.path = $1 AND b.signed = true UNION ALL SELECT 1 \
     FROM builds WHERE build_output_path = $1 AND signed = true)",
  )
  .bind(store_path)
  .fetch_one(pool)
  .await
  .map_err(|e| ApiError(circus_common::CiError::Database(e)))
}

async fn is_servable_harmonia_path(
  pool: &PgPool,
  info: &ValidPathInfo,
) -> Result<bool, ApiError> {
  if info.info.ca.is_some() {
    return Ok(true);
  }
  if info.info.signatures.is_empty() {
    return Ok(false);
  }

  // Do not rebroadcast arbitrary signed paths from the local Nix store. For
  // non-CA paths, serving stays limited to paths Circus built and marked
  // signed.
  let store_path = info.info.store_dir.display(&info.path).to_string();
  has_circus_signed_build_product(pool, &store_path).await
}

/// Serve `NARInfo` for a store path hash.
/// GET /nix-cache/{hash}.narinfo
async fn narinfo(
  State(state): State<AppState>,
  Path(hash): Path<String>,
) -> Result<Response, ApiError> {
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

  let Some(nix_store_db) = open_nix_store_db(&state).await else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };
  let store_dir = state.nix_store.store_dir();
  let Some(info) =
    query_harmonia_path_info(hash, &store_dir, nix_store_db).await?
  else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };

  if !is_servable_harmonia_path(&state.pool, &info).await? {
    return Ok(StatusCode::NOT_FOUND.into_response());
  }

  let store_dir = info.info.store_dir.clone();
  let narinfo = build_narinfo(&store_dir, info, hash, &[]);
  let narinfo_text =
    String::from_utf8(format_narinfo_txt(&store_dir, &narinfo))
      .map_err(cache_data_error)?;

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

/// Serve an uncompressed NAR file. Harmonia narinfos point here as
/// `nar/<nar-hash>.nar?hash=<output-hash>`; legacy Circus URLs using the
/// output hash in the path continue to work for uncached clients.
async fn serve_nar_combined(
  State(state): State<AppState>,
  Path(hash): Path<String>,
  Query(query): Query<NarQuery>,
) -> Result<Response, ApiError> {
  if !state.config.cache.enabled {
    return Ok(StatusCode::NOT_FOUND.into_response());
  }

  if let Some(response) = redirect_uploaded_nar(&state, &hash).await? {
    return Ok(response);
  }

  let Some(stripped) = hash.strip_suffix(".nar") else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };

  let output_hash = query.hash.as_deref().unwrap_or(stripped);
  if !circus_common::validate::is_valid_nix_hash(output_hash) {
    return Ok(StatusCode::NOT_FOUND.into_response());
  }

  let Some(nix_store_db) = open_nix_store_db(&state).await else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };
  let store_dir = state.nix_store.store_dir();
  let Some(info) =
    query_harmonia_path_info(output_hash, &store_dir, nix_store_db).await?
  else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };

  if !is_servable_harmonia_path(&state.pool, &info).await? {
    return Ok(StatusCode::NOT_FOUND.into_response());
  }

  if query.hash.is_some() {
    let nar_hash: Hash = info.info.nar_hash.into();
    let expected_hash = nar_hash.as_base32().as_bare().to_string();
    if stripped != expected_hash {
      return Ok(StatusCode::NOT_FOUND.into_response());
    }
  }

  let store_path =
    PathBuf::from(info.info.store_dir.display(&info.path).to_string());
  let body = Body::from_stream(NarByteStream::new(store_path));

  Ok(
    (
      StatusCode::OK,
      [("content-type", "application/x-nix-nar")],
      body,
    )
      .into_response(),
  )
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
