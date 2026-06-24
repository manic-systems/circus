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
use uuid::Uuid;

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

#[derive(Clone, Copy)]
enum CacheScope {
  Global,
  Project(Uuid),
}

impl CacheScope {
  const fn project_id(self) -> Option<Uuid> {
    match self {
      Self::Global => None,
      Self::Project(id) => Some(id),
    }
  }

  fn cache_key(self, hash: &str) -> String {
    match self {
      Self::Global => format!("global:{hash}"),
      Self::Project(id) => format!("project:{id}:{hash}"),
    }
  }
}

struct CacheSettings {
  scope:      CacheScope,
  enabled:    bool,
  /// Cache name for traffic accounting: `global` or the project name.
  cache_name: String,
}

impl CacheSettings {
  fn global(config: &circus_config::Config) -> Self {
    Self {
      scope:      CacheScope::Global,
      enabled:    config.cache.enabled,
      cache_name: "global".to_owned(),
    }
  }
}

async fn project_cache_settings(
  state: &AppState,
  project_name: &str,
) -> Result<CacheSettings, ApiError> {
  let project =
    circus_common::repo::projects::get_by_name(&state.pool, project_name)
      .await
      .map_err(ApiError)?;
  Ok(CacheSettings {
    scope:      CacheScope::Project(project.id),
    enabled:    project.cache_enabled,
    cache_name: project_name.to_owned(),
  })
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

/// Whether `store_path` is a path Circus itself produced: a recorded build
/// output or build product. This is the provenance check the unauthenticated
/// cache gates on. A path's mere presence in the local store does not make
/// it ours to publish.
async fn has_circus_build_product(
  pool: &PgPool,
  store_path: &str,
  project_id: Option<Uuid>,
) -> Result<bool, ApiError> {
  sqlx::query_scalar::<_, bool>(
    "SELECT EXISTS(SELECT 1 FROM build_products bp JOIN builds b ON b.id = \
     bp.build_id JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j \
     ON j.id = e.jobset_id WHERE bp.path = $1 AND ($2::uuid IS NULL OR \
     j.project_id = $2) UNION ALL SELECT 1 FROM builds b JOIN evaluations e \
     ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE \
     b.build_output_path = $1 AND ($2::uuid IS NULL OR j.project_id = $2))",
  )
  .bind(store_path)
  .bind(project_id)
  .fetch_one(pool)
  .await
  .map_err(|e| ApiError(circus_common::CiError::Database(e)))
}

async fn has_signed_persisted_narinfo(
  pool: &PgPool,
  store_path: &str,
  project_id: Option<Uuid>,
) -> Result<bool, ApiError> {
  sqlx::query_scalar::<_, bool>(
    "SELECT EXISTS(SELECT 1 FROM narinfo_cache WHERE store_path = $1 AND sig \
     IS NOT NULL AND btrim(sig) != '' AND ($2::uuid IS NULL OR project_id = \
     $2))",
  )
  .bind(store_path)
  .bind(project_id)
  .fetch_one(pool)
  .await
  .map_err(|e| ApiError(circus_common::CiError::Database(e)))
}

/// As [`has_circus_build_product`], but additionally requires that the build
/// was signed by Circus.
async fn has_circus_signed_build_product(
  pool: &PgPool,
  store_path: &str,
  project_id: Option<Uuid>,
) -> Result<bool, ApiError> {
  sqlx::query_scalar::<_, bool>(
    "SELECT EXISTS(SELECT 1 FROM build_products bp JOIN builds b ON b.id = \
     bp.build_id JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j \
     ON j.id = e.jobset_id WHERE bp.path = $1 AND b.signed = true AND \
     ($2::uuid IS NULL OR j.project_id = $2) UNION ALL SELECT 1 FROM builds b \
     JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = \
     e.jobset_id WHERE b.build_output_path = $1 AND b.signed = true AND \
     ($2::uuid IS NULL OR j.project_id = $2))",
  )
  .bind(store_path)
  .bind(project_id)
  .fetch_one(pool)
  .await
  .map_err(|e| ApiError(circus_common::CiError::Database(e)))
}

async fn has_circus_derivation_path(
  pool: &PgPool,
  store_path: &str,
  project_id: Option<Uuid>,
) -> Result<bool, ApiError> {
  sqlx::query_scalar::<_, bool>(
    "SELECT EXISTS(SELECT 1 FROM builds b JOIN evaluations e ON e.id = \
     b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE b.drv_path = \
     $1 AND ($2::uuid IS NULL OR j.project_id = $2))",
  )
  .bind(store_path)
  .bind(project_id)
  .fetch_one(pool)
  .await
  .map_err(|e| ApiError(circus_common::CiError::Database(e)))
}

async fn has_circus_derivation_direct_reference(
  pool: &PgPool,
  nix_store_db: &SqlitePool,
  store_path: &str,
  project_id: Option<Uuid>,
) -> Result<bool, ApiError> {
  let referrer_drvs = sqlx::query_scalar::<_, String>(
    "SELECT referrer.path FROM Refs r JOIN ValidPaths requested ON \
     r.reference = requested.id JOIN ValidPaths referrer ON r.referrer = \
     referrer.id WHERE requested.path = ?1 AND referrer.path LIKE '%.drv'",
  )
  .bind(store_path)
  .fetch_all(nix_store_db)
  .await
  .map_err(|e| ApiError(circus_common::CiError::Database(e)))?;

  if referrer_drvs.is_empty() {
    return Ok(false);
  }

  sqlx::query_scalar::<_, bool>(
    "SELECT EXISTS(SELECT 1 FROM builds b JOIN evaluations e ON e.id = \
     b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE b.drv_path = \
     ANY($1) AND ($2::uuid IS NULL OR j.project_id = $2))",
  )
  .bind(referrer_drvs)
  .bind(project_id)
  .fetch_one(pool)
  .await
  .map_err(|e| ApiError(circus_common::CiError::Database(e)))
}

async fn is_servable_harmonia_path(
  pool: &PgPool,
  nix_store_db: &SqlitePool,
  info: &ValidPathInfo,
  scope: CacheScope,
) -> Result<bool, ApiError> {
  // The unauthenticated cache only rebroadcasts paths Circus built, never
  // arbitrary store paths.
  let store_path = info.info.store_dir.display(&info.path).to_string();

  // A dispatched build's own .drv, which agents substitute from this cache to
  // start the build.
  if PathBuf::from(&store_path)
    .extension()
    .is_some_and(|ext| ext.eq_ignore_ascii_case("drv"))
    && has_circus_derivation_path(pool, &store_path, scope.project_id()).await?
  {
    return Ok(true);
  }
  if has_signed_persisted_narinfo(pool, &store_path, scope.project_id()).await?
  {
    return Ok(true);
  }
  if info.info.ca.is_some() {
    // Serve when Circus built it or when it is a direct input of an evaluated
    // derivation Circus may dispatch to an agent.
    return Ok(
      has_circus_build_product(pool, &store_path, scope.project_id()).await?
        || has_circus_derivation_direct_reference(
          pool,
          nix_store_db,
          &store_path,
          scope.project_id(),
        )
        .await?,
    );
  }
  // Non-CA paths are useless to clients without our signature.
  if info.info.signatures.is_empty() {
    return Ok(false);
  }
  has_circus_signed_build_product(pool, &store_path, scope.project_id()).await
}

/// Serve `NARInfo` for a store path hash.
/// GET /nix-cache/{hash}.narinfo
async fn narinfo(
  State(state): State<AppState>,
  Path(hash): Path<String>,
) -> Result<Response, ApiError> {
  let settings = CacheSettings::global(&state.config);
  narinfo_for_settings(state, settings, hash).await
}

async fn project_narinfo(
  State(state): State<AppState>,
  Path((project, hash)): Path<(String, String)>,
) -> Result<Response, ApiError> {
  let settings = project_cache_settings(&state, &project).await?;
  narinfo_for_settings(state, settings, hash).await
}

async fn narinfo_for_settings(
  state: AppState,
  settings: CacheSettings,
  hash: String,
) -> Result<Response, ApiError> {
  if !settings.enabled {
    return Ok(StatusCode::NOT_FOUND.into_response());
  }

  // Strip .narinfo suffix if present
  let hash = hash.strip_suffix(".narinfo").unwrap_or(&hash);

  if !circus_nix::NixHash::is_valid(hash) {
    return Ok(StatusCode::NOT_FOUND.into_response());
  }

  let cache_key = settings.scope.cache_key(hash);
  if let Some(cached) = state.narinfo_cache.get(&cache_key) {
    state.record_cache_serve(&settings.cache_name, cached.len() as u64);
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
  let row = circus_common::repo::narinfo_cache::get_by_hash_part(
    &state.pool,
    hash,
    settings.scope.project_id(),
  )
  .await;
  if let Ok(row) = row
    && narinfo_has_signature(&row)
  {
    let body = render_narinfo_row(&row);
    state.narinfo_cache.insert(cache_key, body.clone());
    state.record_cache_serve(&settings.cache_name, body.len() as u64);
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

  if !is_servable_harmonia_path(
    &state.pool,
    nix_store_db,
    &info,
    settings.scope,
  )
  .await?
  {
    return Ok(StatusCode::NOT_FOUND.into_response());
  }

  let store_dir = info.info.store_dir.clone();
  let narinfo = build_narinfo(&store_dir, info, hash, &[]);
  let narinfo_text =
    String::from_utf8(format_narinfo_txt(&store_dir, &narinfo))
      .map_err(cache_data_error)?;

  state
    .narinfo_cache
    .insert(settings.scope.cache_key(hash), narinfo_text.clone());
  state.record_cache_serve(&settings.cache_name, narinfo_text.len() as u64);

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
  config: &circus_config::Config,
) -> Option<circus_s3::Presigner> {
  let uri = config.cache_upload.store_uri.as_deref()?;
  let s3 = config.cache_upload.s3.as_ref()?;
  circus_s3::Presigner::from_config(uri, s3)
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
  cache_name: &str,
  scope: CacheScope,
) -> Result<Option<Response>, ApiError> {
  if !is_valid_nar_object_name(object_name) {
    return Ok(None);
  }

  let url = format!("nar/{object_name}");
  let row = circus_common::repo::narinfo_cache::get_by_url(
    &state.pool,
    &url,
    scope.project_id(),
  )
  .await;
  match row {
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
      // Bytes leave via S3, but the request is ours to count; bill the known
      // compressed size and stamp the served path's last-fetch time.
      let served = row.file_size.unwrap_or(row.nar_size).max(0);
      state.record_cache_serve(cache_name, served as u64);
      spawn_touch_last_fetched(state, row.store_path);
      let mut response = StatusCode::TEMPORARY_REDIRECT.into_response();
      response.headers_mut().insert(header::LOCATION, location);
      Ok(Some(response))
    },
    Err(circus_common::CiError::NotFound(_)) => Ok(None),
    Err(e) => Err(ApiError(e)),
  }
}

/// Fire-and-forget best-effort `last_fetched_at` stamp for a served path. Never
/// blocks or fails the response; mirrors `touch_api_key_last_used`.
fn spawn_touch_last_fetched(state: &AppState, store_path: String) {
  let pool = state.pool.clone();
  tokio::spawn(async move {
    if let Err(error) =
      circus_common::repo::narinfo_cache::touch_last_fetched(&pool, &store_path)
        .await
    {
      tracing::debug!(%error, store_path, "failed to stamp last_fetched_at");
    }
  });
}

/// Serve an uncompressed NAR file. Harmonia narinfos point here as
/// `nar/<nar-hash>.nar?hash=<output-hash>`; legacy Circus URLs using the
/// output hash in the path continue to work for uncached clients.
async fn serve_nar_combined(
  State(state): State<AppState>,
  Path(hash): Path<String>,
  Query(query): Query<NarQuery>,
) -> Result<Response, ApiError> {
  let settings = CacheSettings::global(&state.config);
  serve_nar_for_settings(state, settings, hash, query).await
}

async fn project_serve_nar_combined(
  State(state): State<AppState>,
  Path((project, hash)): Path<(String, String)>,
  Query(query): Query<NarQuery>,
) -> Result<Response, ApiError> {
  let settings = project_cache_settings(&state, &project).await?;
  serve_nar_for_settings(state, settings, hash, query).await
}

async fn serve_nar_for_settings(
  state: AppState,
  settings: CacheSettings,
  hash: String,
  query: NarQuery,
) -> Result<Response, ApiError> {
  if !settings.enabled {
    return Ok(StatusCode::NOT_FOUND.into_response());
  }

  if let Some(response) =
    redirect_uploaded_nar(&state, &hash, &settings.cache_name, settings.scope)
      .await?
  {
    return Ok(response);
  }

  let Some(stripped) = hash.strip_suffix(".nar") else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };

  let output_hash = query.hash.as_deref().unwrap_or(stripped);
  if !circus_nix::NixHash::is_valid(output_hash) {
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

  if !is_servable_harmonia_path(
    &state.pool,
    nix_store_db,
    &info,
    settings.scope,
  )
  .await?
  {
    return Ok(StatusCode::NOT_FOUND.into_response());
  }

  if query.hash.is_some() {
    let nar_hash: Hash = info.info.nar_hash.into();
    let expected_hash = nar_hash.as_base32().as_bare().to_string();
    if stripped != expected_hash {
      return Ok(StatusCode::NOT_FOUND.into_response());
    }
  }

  let store_path_str = info.info.store_dir.display(&info.path).to_string();
  let nar_size = info.info.nar_size;
  let store_path = PathBuf::from(&store_path_str);
  let body = Body::from_stream(NarByteStream::new(store_path));

  state.record_cache_serve(&settings.cache_name, nar_size);
  spawn_touch_last_fetched(&state, store_path_str);

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
/// GET /nix-cache, /nix-cache/, /nix-cache/nix-cache-info
async fn cache_info(State(state): State<AppState>) -> Response {
  let settings = CacheSettings::global(&state.config);
  cache_info_for_settings(&state, &settings)
}

async fn project_cache_info(
  State(state): State<AppState>,
  Path(project): Path<String>,
) -> Result<Response, ApiError> {
  let settings = project_cache_settings(&state, &project).await?;
  Ok(cache_info_for_settings(&state, &settings))
}

fn cache_info_for_settings(
  state: &AppState,
  settings: &CacheSettings,
) -> Response {
  if !settings.enabled {
    return StatusCode::NOT_FOUND.into_response();
  }

  let store_dir = state.config.nix.store_dir.display();
  let info = format!("StoreDir: {store_dir}\nWantMassQuery: 1\nPriority: 30\n");

  (StatusCode::OK, [("content-type", "text/plain")], info).into_response()
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/nix-cache", get(cache_info))
    .route("/nix-cache/", get(cache_info))
    .route("/nix-cache/nix-cache-info", get(cache_info))
    .route("/nix-cache/{hash}", get(narinfo))
    .route("/nix-cache/nar/{hash}", get(serve_nar_combined))
    .route("/projects/{project}/nix-cache", get(project_cache_info))
    .route("/projects/{project}/nix-cache/", get(project_cache_info))
    .route(
      "/projects/{project}/nix-cache/nix-cache-info",
      get(project_cache_info),
    )
    .route("/projects/{project}/nix-cache/{hash}", get(project_narinfo))
    .route(
      "/projects/{project}/nix-cache/nar/{hash}",
      get(project_serve_nar_combined),
    )
}
