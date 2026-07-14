use std::{
  collections::BTreeSet,
  num::NonZero,
  path::{self, PathBuf},
  time::Duration,
};

use axum::{
  Router,
  body::Body,
  extract::{Path, Query, State},
  http::{HeaderValue, StatusCode, header},
  response::{IntoResponse, Response},
  routing::get,
};
use circus_binary_cache::{
  ContentAddress,
  Hash,
  HashFormat as _,
  NarByteStream,
  NarHash,
  PublicKey,
  Signature,
  StoreDir,
  StorePath,
  StorePathHash,
  UnkeyedValidPathInfo,
  ValidPathInfo,
  build_narinfo,
  fmt::Any as AnyHashFmt,
  format_narinfo_txt,
};
use circus_common::{PgPool, repo::cache::PersistedNarinfoSig};
use serde::Deserialize;
use tokio_rusqlite::{
  Connection as SqliteConnection,
  OptionalExtension,
  rusqlite,
};
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

const S3_GET_PRESIGN_EXPIRY: Duration = Duration::from_hours(1);
const MAX_NAR_OBJECT_NAME_LEN: usize = 512;

struct ValidPathRow {
  id:                i64,
  path:              String,
  nar_hash:          String,
  registration_time: i64,
  deriver:           Option<String>,
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

/// The local Nix store DB is sqlite, so its errors cannot flow through
/// `CiError::Database` (which wraps tokio-postgres errors).
fn nix_store_db_error(error: impl std::fmt::Display) -> ApiError {
  ApiError(circus_common::CiError::Internal(format!(
    "local Nix store DB query failed: {error}"
  )))
}

async fn open_nix_store_db(state: &AppState) -> Option<&SqliteConnection> {
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

/// Whether a persisted narinfo row may be served.
///
/// With a loaded public key the signature must verify cryptographically.
/// Without one we cannot verify anything, so fall back to the historical
/// non-empty signature check.
fn persisted_row_is_trusted(
  row: &circus_common::repo::narinfo_cache::NarInfo,
  public_key: Option<&PublicKey>,
) -> bool {
  let Some(public_key) = public_key else {
    static WARNED: std::sync::Once = std::sync::Once::new();
    WARNED.call_once(|| {
      tracing::warn!(
        "serving persisted narinfos without signature verification because no \
         signing public key is loaded"
      );
    });
    return narinfo_has_signature(row);
  };
  let sig_row = PersistedNarinfoSig {
    nar_hash:   row.nar_hash.clone(),
    nar_size:   row.nar_size,
    references: row.references.clone(),
    sig:        row.sig.clone(),
  };
  narinfo_signature_verifies(&row.store_path, &sig_row, public_key)
}

fn narinfo_row_uses_local_nar_route(
  row: &circus_common::repo::narinfo_cache::NarInfo,
) -> bool {
  if row.compression != "none" {
    return false;
  }
  let Some(path) = row.url.strip_prefix("nar/") else {
    return false;
  };
  let Some((object, query)) = path.split_once('?') else {
    return false;
  };
  let object = path::Path::new(object);
  let has_nar_ext = object
    .extension()
    .is_some_and(|ext| ext.eq_ignore_ascii_case("nar"));
  let stem_nonempty = object
    .file_stem()
    .and_then(|s| s.to_str())
    .is_some_and(|s| !s.is_empty());
  // Require a real store hash in `hash=`, not just the URL shape.
  let has_valid_output_hash = query
    .split('&')
    .filter_map(|part| part.strip_prefix("hash="))
    .any(circus_nix::NixHash::is_valid);
  has_nar_ext && stem_nonempty && has_valid_output_hash
}

async fn local_narinfo_row_is_servable(
  state: &AppState,
  hash: &str,
  scope: CacheScope,
) -> Result<bool, ApiError> {
  let Some(nix_store_db) = open_nix_store_db(state).await else {
    return Ok(false);
  };
  let store_dir = state.nix_store.store_dir();
  let Some(info) =
    query_binary_cache_path_info(hash, &store_dir, nix_store_db).await?
  else {
    return Ok(false);
  };
  is_servable_binary_cache_path(
    &state.pool,
    nix_store_db,
    &info,
    scope,
    state.cache_public_key.as_deref(),
  )
  .await
}

async fn query_binary_cache_path_info(
  hash: &str,
  store_dir: &StoreDir,
  nix_store_db: &SqliteConnection,
) -> Result<Option<ValidPathInfo>, ApiError> {
  let Ok(hash) = StorePathHash::decode_digest(hash.as_bytes()) else {
    return Ok(None);
  };
  let prefix = format!("{store_dir}/{hash}");
  let query_prefix = prefix.clone();

  let Some((row, references)) = nix_store_db
    .call(
      move |conn| -> rusqlite::Result<Option<(ValidPathRow, Vec<String>)>> {
        let row = conn
          .query_row(
            "SELECT id, path, hash, registrationTime, deriver, narSize, \
             ultimate, sigs, ca FROM ValidPaths WHERE path >= ?1 ORDER BY \
             path ASC LIMIT 1",
            [&query_prefix],
            |row| {
              Ok(ValidPathRow {
                id:                row.get(0)?,
                path:              row.get(1)?,
                nar_hash:          row.get(2)?,
                registration_time: row.get(3)?,
                deriver:           row.get(4)?,
                nar_size:          row.get(5)?,
                ultimate:          row.get(6)?,
                sigs:              row.get(7)?,
                ca:                row.get(8)?,
              })
            },
          )
          .optional()?;
        let Some(row) = row else {
          return Ok(None);
        };

        let mut statement = conn.prepare(
          "SELECT v.path FROM Refs r JOIN ValidPaths v ON r.reference = v.id \
           WHERE r.referrer = ?1",
        )?;
        let references = statement
          .query_map([row.id], |row| row.get(0))?
          .collect::<Result<Vec<String>, _>>()?;
        Ok(Some((row, references)))
      },
    )
    .await
    .map_err(nix_store_db_error)?
  else {
    return Ok(None);
  };

  if !row.path.starts_with(&prefix) {
    return Ok(None);
  }

  let references = references
    .into_iter()
    .filter_map(|path| store_dir.parse(&path).ok())
    .collect::<BTreeSet<_>>();

  let Ok(path) = store_dir.parse::<StorePath>(&row.path) else {
    return Ok(None);
  };
  let deriver = row
    .deriver
    .and_then(|path| store_dir.parse::<StorePath>(&path).ok());
  let Ok(nar_hash) = row.nar_hash.parse::<AnyHashFmt<NarHash>>() else {
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
  circus_common::repo::cache::has_circus_build_product(
    pool, store_path, project_id,
  )
  .await
  .map_err(ApiError)
}

/// Whether a persisted narinfo's signature verifies under our own public key.
///
/// Without a loaded key this falls back to the historical non-empty signature
/// check the query already applies.
async fn has_signed_persisted_narinfo(
  pool: &PgPool,
  store_path: &str,
  project_id: Option<Uuid>,
  public_key: Option<&PublicKey>,
) -> Result<bool, ApiError> {
  let row = circus_common::repo::cache::signed_persisted_narinfo(
    pool, store_path, project_id,
  )
  .await
  .map_err(ApiError)?;
  let Some(public_key) = public_key else {
    return Ok(row.is_some());
  };
  Ok(row.is_some_and(|row| {
    narinfo_signature_verifies(store_path, &row, public_key)
  }))
}

/// Verify a persisted narinfo signature against the same fingerprint the runner
/// signed.
fn narinfo_signature_verifies(
  store_path: &str,
  row: &PersistedNarinfoSig,
  public_key: &PublicKey,
) -> bool {
  let Some(signature) =
    row.sig.as_deref().and_then(|s| s.parse::<Signature>().ok())
  else {
    return false;
  };
  let mut references = row.references.clone();
  references.sort();
  let fingerprint = format!(
    "1;{store_path};{};{};{}",
    row.nar_hash,
    row.nar_size,
    references.join(",")
  );
  public_key.verify(fingerprint.as_bytes(), &signature)
}

/// As [`has_circus_build_product`], but additionally requires that the build
/// was signed by Circus.
async fn has_circus_signed_build_product(
  pool: &PgPool,
  store_path: &str,
  project_id: Option<Uuid>,
) -> Result<bool, ApiError> {
  circus_common::repo::cache::has_circus_signed_build_product(
    pool, store_path, project_id,
  )
  .await
  .map_err(ApiError)
}

async fn has_circus_derivation_path(
  pool: &PgPool,
  store_path: &str,
  project_id: Option<Uuid>,
) -> Result<bool, ApiError> {
  circus_common::repo::cache::has_circus_derivation_path(
    pool, store_path, project_id,
  )
  .await
  .map_err(ApiError)
}

async fn has_circus_derivation_direct_reference(
  pool: &PgPool,
  nix_store_db: &SqliteConnection,
  store_path: &str,
  project_id: Option<Uuid>,
) -> Result<bool, ApiError> {
  let store_path = store_path.to_owned();
  let referrer_drvs = nix_store_db
    .call(move |conn| -> rusqlite::Result<Vec<String>> {
      let mut statement = conn.prepare(
        "SELECT referrer.path FROM Refs r JOIN ValidPaths requested ON \
         r.reference = requested.id JOIN ValidPaths referrer ON r.referrer = \
         referrer.id WHERE requested.path = ?1 AND referrer.path LIKE '%.drv'",
      )?;
      statement
        .query_map([store_path], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()
    })
    .await
    .map_err(nix_store_db_error)?;

  if referrer_drvs.is_empty() {
    return Ok(false);
  }

  circus_common::repo::cache::has_circus_derivation_path_any(
    pool,
    &referrer_drvs,
    project_id,
  )
  .await
  .map_err(ApiError)
}

async fn is_servable_binary_cache_path(
  pool: &PgPool,
  nix_store_db: &SqliteConnection,
  info: &ValidPathInfo,
  scope: CacheScope,
  public_key: Option<&PublicKey>,
) -> Result<bool, ApiError> {
  // The unauthenticated cache only rebroadcasts paths Circus built, never
  // arbitrary store paths.
  let store_path = info.info.store_dir.display(&info.path).to_string();
  if has_signed_persisted_narinfo(
    pool,
    &store_path,
    scope.project_id(),
    public_key,
  )
  .await?
  {
    return Ok(true);
  }
  // A dispatched build's own .drv: agents substitute it from this cache to
  // start the build. Derivations are content-addressed, so this must be
  // checked before the generic CA branch below, which only covers build
  // outputs and their direct inputs, never the derivation file itself.
  if PathBuf::from(&store_path)
    .extension()
    .is_some_and(|ext| ext.eq_ignore_ascii_case("drv"))
    && has_circus_derivation_path(pool, &store_path, scope.project_id()).await?
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
    && persisted_row_is_trusted(&row, state.cache_public_key.as_deref())
  {
    let local_nar_route = narinfo_row_uses_local_nar_route(&row);
    if local_nar_route
      && !local_narinfo_row_is_servable(&state, hash, settings.scope).await?
    {
      return Ok(StatusCode::NOT_FOUND.into_response());
    }
    let body = render_narinfo_row(&row);
    if !local_nar_route {
      state.narinfo_cache.insert(cache_key, body.clone());
    }
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
    query_binary_cache_path_info(hash, &store_dir, nix_store_db).await?
  else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };

  if !is_servable_binary_cache_path(
    &state.pool,
    nix_store_db,
    &info,
    settings.scope,
    state.cache_public_key.as_deref(),
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
  let references = row
    .references
    .iter()
    .map(|path| store_path_name(path))
    .collect::<Vec<_>>()
    .join(" ");
  let _ = writeln!(s, "References: {references}");
  if let Some(d) = &row.deriver {
    let _ = writeln!(s, "Deriver: {}", store_path_name(d));
  }
  if let Some(c) = &row.ca {
    let _ = writeln!(s, "CA: {c}");
  }
  if let Some(sig) = &row.sig {
    let _ = writeln!(s, "Sig: {sig}");
  }
  s
}

fn store_path_name(path: &str) -> &str {
  path
    .rsplit('/')
    .find(|part| !part.is_empty())
    .unwrap_or(path)
}

fn store_path_hash_part(path: &str) -> Option<&str> {
  let (hash, _) = store_path_name(path).split_once('-')?;
  Some(hash)
}

fn nar_hash_part(nar_hash: &str) -> Option<&str> {
  nar_hash.strip_prefix("sha256:")
}

fn persisted_local_nar_row_matches(
  row: &circus_common::repo::narinfo_cache::NarInfo,
  output_hash: &str,
  nar_hash: &str,
) -> bool {
  narinfo_has_signature(row)
    && row.compression == "none"
    && store_path_hash_part(&row.store_path) == Some(output_hash)
    && nar_hash_part(&row.nar_hash) == Some(nar_hash)
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

async fn serve_persisted_local_nar(
  state: &AppState,
  object_name: &str,
  output_hash: &str,
  nar_hash: &str,
  cache_name: &str,
  scope: CacheScope,
) -> Result<Option<Response>, ApiError> {
  if !is_valid_nar_object_name(object_name) {
    return Ok(None);
  }

  let url = format!("nar/{object_name}?hash={output_hash}");
  let row = circus_common::repo::narinfo_cache::get_by_url(
    &state.pool,
    &url,
    scope.project_id(),
  )
  .await;
  let row = match row {
    Ok(row) => row,
    Err(circus_common::CiError::NotFound(_)) => return Ok(None),
    Err(e) => return Err(ApiError(e)),
  };

  if !persisted_local_nar_row_matches(&row, output_hash, nar_hash) {
    return Ok(Some(StatusCode::NOT_FOUND.into_response()));
  }

  if tokio::fs::metadata(&row.store_path).await.is_err() {
    return Ok(Some(StatusCode::NOT_FOUND.into_response()));
  }

  let nar_size = row.nar_size.max(0) as u64;
  Ok(Some(serve_nar_from_store_path(
    state,
    row.store_path,
    nar_size,
    cache_name,
  )))
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

/// Stream a NAR straight from a store path, recording the serve and stamping
/// `last_fetched_at`.
fn serve_nar_from_store_path(
  state: &AppState,
  store_path: String,
  nar_size: u64,
  cache_name: &str,
) -> Response {
  let body = Body::from_stream(NarByteStream::new(PathBuf::from(&store_path)));
  state.record_cache_serve(cache_name, nar_size);
  spawn_touch_last_fetched(state, store_path);
  (
    StatusCode::OK,
    [("content-type", "application/x-nix-nar")],
    body,
  )
    .into_response()
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

  if let Some(output_hash) = query.hash.as_deref()
    && let Some(response) = serve_persisted_local_nar(
      &state,
      &hash,
      output_hash,
      stripped,
      &settings.cache_name,
      settings.scope,
    )
    .await?
  {
    return Ok(response);
  }

  let Some(nix_store_db) = open_nix_store_db(&state).await else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };
  let store_dir = state.nix_store.store_dir();
  let Some(info) =
    query_binary_cache_path_info(output_hash, &store_dir, nix_store_db).await?
  else {
    return Ok(StatusCode::NOT_FOUND.into_response());
  };

  if !is_servable_binary_cache_path(
    &state.pool,
    nix_store_db,
    &info,
    settings.scope,
    state.cache_public_key.as_deref(),
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
  Ok(serve_nar_from_store_path(
    &state,
    store_path_str,
    info.info.nar_size,
    &settings.cache_name,
  ))
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

#[cfg(test)]
mod tests {
  use chrono::Utc;
  use circus_common::repo::narinfo_cache::NarInfo;

  use super::*;

  fn test_narinfo_row() -> circus_common::repo::narinfo_cache::NarInfo {
    let now = Utc::now();
    circus_common::repo::narinfo_cache::NarInfo {
      store_path:      "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-cache-test"
        .to_owned(),
      nar_hash:
        "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_owned(),
      nar_size:        1,
      file_hash:       None,
      file_size:       None,
      compression:     "none".to_owned(),
      url:             "nar/cache-test.nar".to_owned(),
      deriver:         Some(
        "/nix/store/cccccccccccccccccccccccccccccccc-cache-test.drv".to_owned(),
      ),
      references:      vec![
        "/nix/store/dddddddddddddddddddddddddddddddd-glibc".to_owned(),
        "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee-zlib".to_owned(),
      ],
      sig:             Some("circus:test-signature".to_owned()),
      ca:              None,
      build_id:        None,
      project_id:      None,
      created_at:      now,
      updated_at:      now,
      last_fetched_at: None,
    }
  }

  #[test]
  fn render_narinfo_row_uses_store_path_names_for_refs_and_deriver() {
    let body = render_narinfo_row(&test_narinfo_row());
    assert!(
      body.contains(
        "References: dddddddddddddddddddddddddddddddd-glibc \
         eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee-zlib\n"
      ),
      "{body}"
    );
    assert!(
      body
        .contains("Deriver: cccccccccccccccccccccccccccccccc-cache-test.drv\n"),
      "{body}"
    );
  }

  #[test]
  fn persisted_local_nar_row_requires_a_signature() {
    let row = test_narinfo_row();
    let hash = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let nar = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    assert!(persisted_local_nar_row_matches(&row, hash, nar));

    let unsigned = NarInfo { sig: None, ..row };
    assert!(!persisted_local_nar_row_matches(&unsigned, hash, nar));
  }

  #[test]
  fn persisted_narinfo_signature_must_verify_against_our_key() {
    use circus_binary_cache::SecretKey;

    let secret = "circus-test-1:\
                  OlzHrxDxaOpPjkL5uNXF77Xq4VRiz6Zy0LqlK6GCNqRX90gxFy2HSr/\
                  hxqdpc2VMU2UIlDOAEBv842MCsbPfgQ=="
      .parse::<SecretKey>()
      .expect("valid test secret key");
    let public = secret.to_public_key();
    let store_path = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-cache-test";
    let nar_hash =
      "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let references = vec![
      "/nix/store/dddddddddddddddddddddddddddddddd-glibc".to_owned(),
      "/nix/store/cccccccccccccccccccccccccccccccc-zlib".to_owned(),
    ];
    let mut sorted = references.clone();
    sorted.sort();
    let fingerprint =
      format!("1;{store_path};{nar_hash};42;{}", sorted.join(","));
    let sig = secret.sign(fingerprint.as_bytes()).to_string();

    let signed = PersistedNarinfoSig {
      nar_hash: nar_hash.to_owned(),
      nar_size: 42,
      references,
      sig: Some(sig),
    };
    assert!(narinfo_signature_verifies(store_path, &signed, &public));

    let tampered = PersistedNarinfoSig {
      nar_size: 43,
      ..signed
    };
    assert!(!narinfo_signature_verifies(store_path, &tampered, &public));
  }

  #[test]
  fn persisted_row_trust_requires_verification_only_with_a_key() {
    use circus_binary_cache::SecretKey;

    let secret = "circus-test-1:\
                  OlzHrxDxaOpPjkL5uNXF77Xq4VRiz6Zy0LqlK6GCNqRX90gxFy2HSr/\
                  hxqdpc2VMU2UIlDOAEBv842MCsbPfgQ=="
      .parse::<SecretKey>()
      .expect("valid test secret key");
    let public = secret.to_public_key();
    let row = NarInfo {
      sig: Some("circus-test-1:bm90IGEgcmVhbCBzaWduYXR1cmU=".to_owned()),
      ..test_narinfo_row()
    };

    assert!(persisted_row_is_trusted(&row, None));
    assert!(!persisted_row_is_trusted(&row, Some(&public)));
  }
}
