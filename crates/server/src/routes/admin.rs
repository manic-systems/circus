use axum::{
  Json,
  Router,
  extract::{Path, Query, State},
  routing::{get, post},
};
use circus_common::{
  PaginatedResponse,
  Validate,
  audit::AuditEntry,
  models::{
    Build,
    CreateRemoteBuilder,
    NotificationTask,
    RemoteBuilder,
    SystemStatus,
    UpdateRemoteBuilder,
  },
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
  auth_middleware::RequireAdmin,
  cache_overview,
  error::ApiError,
  state::AppState,
};

fn config_file_path() -> Option<std::path::PathBuf> {
  std::env::var_os("CIRCUS_CONFIG_FILE").map(std::path::PathBuf::from)
}

async fn list_builders(
  _auth: RequireAdmin,
  State(state): State<AppState>,
) -> Result<Json<Vec<RemoteBuilder>>, ApiError> {
  let builders =
    circus_common::repo::remote_builders::list(&state.pool).await?;
  Ok(Json(builders))
}

/// All builder sessions known to the cluster, connected or not. Backed by
/// the `builder_sessions` table that the queue-runner upserts on register
/// and on heartbeat.
async fn list_builder_sessions(
  _auth: RequireAdmin,
  State(state): State<AppState>,
) -> Result<
  Json<Vec<circus_common::repo::builder_sessions::BuilderSession>>,
  ApiError,
> {
  let sessions =
    circus_common::repo::builder_sessions::list(&state.pool).await?;
  Ok(Json(sessions))
}

/// Currently-connected agents only. The shape of the rows is the same as
/// [`list_builder_sessions`]; this endpoint matches the dashboard's
/// "live agents" panel.
async fn list_connected_builder_sessions(
  _auth: RequireAdmin,
  State(state): State<AppState>,
) -> Result<
  Json<Vec<circus_common::repo::builder_sessions::BuilderSession>>,
  ApiError,
> {
  let sessions =
    circus_common::repo::builder_sessions::list_connected(&state.pool).await?;
  Ok(Json(sessions))
}

async fn get_builder_session(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path(machine_id): Path<Uuid>,
) -> Result<Json<circus_common::repo::builder_sessions::BuilderSession>, ApiError>
{
  let session =
    circus_common::repo::builder_sessions::get(&state.pool, machine_id).await?;
  Ok(Json(session))
}

async fn get_builder(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<RemoteBuilder>, ApiError> {
  let builder =
    circus_common::repo::remote_builders::get(&state.pool, id).await?;
  Ok(Json(builder))
}

async fn create_builder(
  auth: RequireAdmin,
  State(state): State<AppState>,
  Json(input): Json<CreateRemoteBuilder>,
) -> Result<Json<RemoteBuilder>, ApiError> {
  input
    .validate()
    .map_err(|msg| ApiError(circus_common::CiError::Validation(msg)))?;
  let builder =
    circus_common::repo::remote_builders::create(&state.pool, input).await?;

  crate::audit::record_for_key(
    &state.pool,
    &auth.0,
    "BUILDER_CREATE",
    Some("builder"),
    Some(&builder.id.to_string()),
    serde_json::json!({
      "name": builder.name,
      "ssh_uri": builder.ssh_uri,
      "ssh_key_file": builder.ssh_key_file,
      "host_key_pinned": builder.public_host_key.is_some(),
    }),
  )
  .await;

  Ok(Json(builder))
}

async fn update_builder(
  auth: RequireAdmin,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  Json(input): Json<UpdateRemoteBuilder>,
) -> Result<Json<RemoteBuilder>, ApiError> {
  input
    .validate()
    .map_err(|msg| ApiError(circus_common::CiError::Validation(msg)))?;

  // Capture which security-relevant fields the request changed before `input`
  // is moved into the update, so key rotations and host-key changes are
  // auditable.
  let ssh_uri_changed = input.ssh_uri.is_some();
  let ssh_key_file_changed = input.ssh_key_file.is_some();
  let public_host_key_changed = input.public_host_key.is_some();

  let builder =
    circus_common::repo::remote_builders::update(&state.pool, id, input)
      .await?;

  crate::audit::record_for_key(
    &state.pool,
    &auth.0,
    "BUILDER_UPDATE",
    Some("builder"),
    Some(&builder.id.to_string()),
    serde_json::json!({
      "name": builder.name,
      "enabled": builder.enabled,
      "ssh_uri_changed": ssh_uri_changed,
      "ssh_key_file_changed": ssh_key_file_changed,
      "public_host_key_changed": public_host_key_changed,
    }),
  )
  .await;

  Ok(Json(builder))
}

async fn delete_builder(
  auth: RequireAdmin,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
  circus_common::repo::remote_builders::delete(&state.pool, id).await?;

  crate::audit::record_for_key(
    &state.pool,
    &auth.0,
    "BUILDER_DELETE",
    Some("builder"),
    Some(&id.to_string()),
    serde_json::Value::Null,
  )
  .await;

  Ok(Json(serde_json::json!({"deleted": true})))
}

async fn system_status(
  _auth: RequireAdmin,
  State(state): State<AppState>,
) -> Result<Json<SystemStatus>, ApiError> {
  let pool = &state.pool;

  let projects = circus_common::repo::projects::count(pool).await?;
  let jobsets = circus_common::repo::jobsets::count(pool).await?;
  let evaluations = circus_common::repo::evaluations::count(pool).await?;

  let build_stats = circus_common::repo::builds::get_stats(pool).await?;
  let builders = circus_common::repo::remote_builders::count(pool).await?;

  let channels = circus_common::repo::channels::count(pool).await?;

  Ok(Json(SystemStatus {
    projects_count:    projects,
    jobsets_count:     jobsets,
    evaluations_count: evaluations,
    builds_pending:    build_stats.pending_builds.unwrap_or(0),
    builds_running:    build_stats.running_builds.unwrap_or(0),
    builds_completed:  build_stats.completed_builds.unwrap_or(0),
    builds_failed:     build_stats.failed_builds.unwrap_or(0),
    remote_builders:   builders,
    channels_count:    channels,
  }))
}

async fn list_notification_tasks(
  _auth: RequireAdmin,
  State(state): State<AppState>,
) -> Result<Json<Vec<NotificationTask>>, ApiError> {
  let tasks =
    circus_common::repo::notification_tasks::list_recent(&state.pool, 100)
      .await?;
  Ok(Json(tasks))
}

async fn retry_notification_task(
  auth: RequireAdmin,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<NotificationTask>, ApiError> {
  let task =
    circus_common::repo::notification_tasks::requeue_failed(&state.pool, id)
      .await?;

  crate::audit::record_for_key(
    &state.pool,
    &auth.0,
    "NOTIFICATION_TASK_RETRY",
    Some("notification_task"),
    Some(&id.to_string()),
    serde_json::Value::Null,
  )
  .await;

  Ok(Json(task))
}

#[derive(Debug, Deserialize)]
struct PinnedProductsQuery {
  #[serde(default)]
  limit:  Option<i64>,
  #[serde(default)]
  offset: Option<i64>,
}

async fn list_pinned_build_products(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Query(q): Query<PinnedProductsQuery>,
) -> Result<
  Json<
    PaginatedResponse<circus_common::repo::build_products::PinnedBuildProduct>,
  >,
  ApiError,
> {
  let limit = q.limit.unwrap_or(100).clamp(1, 500);
  let offset = q.offset.unwrap_or(0).max(0);
  let items = circus_common::repo::build_products::list_pinned(
    &state.pool,
    limit,
    offset,
  )
  .await?;
  let total =
    circus_common::repo::build_products::count_pinned(&state.pool).await?;

  Ok(Json(PaginatedResponse {
    items,
    total,
    limit,
    offset,
  }))
}

async fn unpin_build(
  auth: RequireAdmin,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Build>, ApiError> {
  let build =
    circus_common::repo::builds::set_keep(&state.pool, id, false).await?;

  crate::audit::record_for_key(
    &state.pool,
    &auth.0,
    "BUILD_UNPIN",
    Some("build"),
    Some(&id.to_string()),
    serde_json::json!({ "job_name": &build.job_name }),
  )
  .await;

  Ok(Json(build))
}

#[derive(Debug, Serialize)]
struct ConfigFileResponse {
  path:             String,
  contents:         String,
  requires_restart: bool,
  editable:         bool,
  read_only_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct UpdateConfigFile {
  contents: String,
}

async fn get_config_file(
  _auth: RequireAdmin,
  State(state): State<AppState>,
) -> Result<Json<ConfigFileResponse>, ApiError> {
  let Some(path) = config_file_path() else {
    return Ok(Json(ConfigFileResponse {
      path:             String::new(),
      contents:         String::new(),
      requires_restart: true,
      editable:         false,
      read_only_reason: Some(
        "CIRCUS_CONFIG_FILE is not set; no config file is available"
          .to_string(),
      ),
    }));
  };
  let contents = match tokio::fs::read_to_string(&path).await {
    Ok(contents) => {
      let parsed = circus_config::Config::from_toml_with_defaults(&contents)
        .map_err(|e| {
          ApiError(circus_common::CiError::Validation(format!(
            "Invalid TOML configuration in {}: {e}",
            path.display()
          )))
        })?;
      let mut value = toml::Value::try_from(&parsed).map_err(|e| {
        ApiError(circus_common::CiError::Internal(format!(
          "Failed to serialize configuration: {e}"
        )))
      })?;
      circus_config::redact_secrets(&mut value);
      toml::to_string_pretty(&value).map_err(|e| {
        ApiError(circus_common::CiError::Internal(format!(
          "Failed to render effective configuration: {e}"
        )))
      })?
    },
    Err(e) => return Err(ApiError(circus_common::CiError::Io(e))),
  };

  Ok(Json(ConfigFileResponse {
    path: path.display().to_string(),
    contents,
    requires_restart: true,
    editable: state.config.server.config_editor_enabled,
    read_only_reason: (!state.config.server.config_editor_enabled).then_some(
      "Config editor is disabled by server configuration".to_string(),
    ),
  }))
}

async fn update_config_file(
  auth: RequireAdmin,
  State(state): State<AppState>,
  Json(input): Json<UpdateConfigFile>,
) -> Result<Json<ConfigFileResponse>, ApiError> {
  if !state.config.server.config_editor_enabled {
    return Err(ApiError(circus_common::CiError::Forbidden(
      "Config editor is disabled by server configuration".to_string(),
    )));
  }

  let parsed = circus_config::Config::from_toml_with_defaults(&input.contents)
    .map_err(|e| {
      ApiError(circus_common::CiError::Validation(format!(
        "Invalid TOML configuration: {e}"
      )))
    })?;
  let rendered = toml::to_string_pretty(&parsed).map_err(|e| {
    ApiError(circus_common::CiError::Internal(format!(
      "Failed to render configuration: {e}"
    )))
  })?;

  let Some(path) = config_file_path() else {
    return Err(ApiError(circus_common::CiError::Forbidden(
      "CIRCUS_CONFIG_FILE is not set; no config file is available".to_string(),
    )));
  };
  let tmp_path = path.with_extension("toml.tmp");
  tokio::fs::write(&tmp_path, &rendered)
    .await
    .map_err(|e| ApiError(circus_common::CiError::Io(e)))?;
  tokio::fs::rename(&tmp_path, &path)
    .await
    .map_err(|e| ApiError(circus_common::CiError::Io(e)))?;

  crate::audit::record_for_key(
    &state.pool,
    &auth.0,
    "CONFIG_UPDATE",
    Some("config"),
    Some(&path.display().to_string()),
    // Body of the config can contain secrets; record only its size and
    // checksum so the log stays useful without leaking credentials.
    serde_json::json!({
      "bytes":  rendered.len(),
    }),
  )
  .await;

  Ok(Json(ConfigFileResponse {
    path:             path.display().to_string(),
    contents:         rendered,
    requires_restart: true,
    editable:         true,
    read_only_reason: None,
  }))
}

#[derive(Debug, Deserialize)]
struct AuditLogQuery {
  #[serde(default)]
  limit:  Option<i64>,
  #[serde(default)]
  offset: Option<i64>,
}

#[derive(Debug, Serialize)]
struct AuditLogPage {
  items:  Vec<AuditEntry>,
  total:  i64,
  limit:  i64,
  offset: i64,
}

async fn list_audit_log(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Query(q): Query<AuditLogQuery>,
) -> Result<Json<AuditLogPage>, ApiError> {
  let limit = q.limit.unwrap_or(50).clamp(1, 500);
  let offset = q.offset.unwrap_or(0).max(0);

  let items = circus_common::audit::list(&state.pool, limit, offset).await?;
  let total = circus_common::audit::count(&state.pool).await?;

  Ok(Json(AuditLogPage {
    items,
    total,
    limit,
    offset,
  }))
}

#[derive(Debug, Serialize)]
struct CacheSummaryItem {
  name:               String,
  scope:              &'static str,
  active:             bool,
  nar_count:          i64,
  compressed_bytes:   i64,
  uncompressed_bytes: i64,
  requests_last_hour: i64,
}

/// List the global cache plus every cache-enabled project with storage and
/// trailing-hour request counts.
async fn list_caches(
  _auth: RequireAdmin,
  State(state): State<AppState>,
) -> Result<Json<Vec<CacheSummaryItem>>, ApiError> {
  let refs = cache_overview::list_cache_refs(&state).await?;
  let mut items = Vec::with_capacity(refs.len());
  for cache in refs {
    let storage = circus_common::repo::narinfo_cache::storage_summary(
      &state.pool,
      cache.scope,
    )
    .await?;
    let (requests_last_hour, _bytes) =
      circus_common::repo::cache_traffic::traffic_last_hour(
        &state.pool,
        &cache.name,
      )
      .await?;
    items.push(CacheSummaryItem {
      scope: cache.scope_kind(),
      name: cache.name,
      active: cache.active,
      nar_count: storage.nar_count,
      compressed_bytes: storage.compressed_bytes,
      uncompressed_bytes: storage.uncompressed_bytes,
      requests_last_hour,
    });
  }
  Ok(Json(items))
}

#[derive(Debug, Serialize)]
struct TrafficLastHour {
  requests:     i64,
  bytes_served: i64,
}

#[derive(Debug, Serialize)]
struct CacheDetailResponse {
  name:              String,
  scope:             &'static str,
  active:            bool,
  substituter_url:   Option<String>,
  public_key:        Option<String>,
  nix_conf_snippet:  Option<String>,
  storage:           circus_common::repo::narinfo_cache::CacheStorageSummary,
  traffic_last_hour: TrafficLastHour,
}

fn cache_not_found(name: &str) -> ApiError {
  ApiError(circus_common::CiError::NotFound(format!("cache {name}")))
}

/// Detail for one cache: substituter URL, public key, ready-to-paste nix.conf
/// snippet, storage totals, and trailing-hour traffic.
async fn get_cache(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path(name): Path<String>,
) -> Result<Json<CacheDetailResponse>, ApiError> {
  let Some(cache) = cache_overview::resolve_cache_ref(&state, &name).await?
  else {
    return Err(cache_not_found(&name));
  };
  let storage = circus_common::repo::narinfo_cache::storage_summary(
    &state.pool,
    cache.scope,
  )
  .await?;
  let (requests, bytes_served) =
    circus_common::repo::cache_traffic::traffic_last_hour(
      &state.pool,
      &cache.name,
    )
    .await?;
  let substituter = cache_overview::substituter_url(&state.config, &cache);
  let public_key = cache_overview::public_key(&state.config);
  let nix_conf_snippet = cache_overview::nix_conf_snippet(
    substituter.as_deref(),
    public_key.as_deref(),
  );
  Ok(Json(CacheDetailResponse {
    scope: cache.scope_kind(),
    name: cache.name,
    active: cache.active,
    substituter_url: substituter,
    public_key,
    nix_conf_snippet,
    storage,
    traffic_last_hour: TrafficLastHour {
      requests,
      bytes_served,
    },
  }))
}

#[derive(Debug, Deserialize)]
struct TimeseriesQuery {
  #[serde(default)]
  granularity: Option<String>,
  #[serde(default)]
  points:      Option<i64>,
}

impl TimeseriesQuery {
  fn resolved(&self) -> (circus_common::repo::cache_traffic::Granularity, i64) {
    let granularity =
      circus_common::repo::cache_traffic::Granularity::from_param(
        self.granularity.as_deref().unwrap_or("hours"),
      );
    let points = self.points.unwrap_or(48).clamp(1, 500);
    (granularity, points)
  }
}

#[derive(Debug, Serialize)]
struct StorageSeries {
  timestamps:     Vec<String>,
  packages_added: Vec<i64>,
  bytes_added:    Vec<i64>,
}

async fn cache_storage_timeseries(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path(name): Path<String>,
  Query(query): Query<TimeseriesQuery>,
) -> Result<Json<StorageSeries>, ApiError> {
  let Some(cache) = cache_overview::resolve_cache_ref(&state, &name).await?
  else {
    return Err(cache_not_found(&name));
  };
  let (granularity, points) = query.resolved();
  let series = circus_common::repo::cache_traffic::storage_timeseries(
    &state.pool,
    cache.scope,
    granularity,
    points,
  )
  .await?;
  Ok(Json(StorageSeries {
    timestamps:     series.iter().map(|p| p.bucket_time.to_rfc3339()).collect(),
    packages_added: series.iter().map(|p| p.packages_added).collect(),
    bytes_added:    series.iter().map(|p| p.bytes_added).collect(),
  }))
}

#[derive(Debug, Serialize)]
struct TrafficSeries {
  timestamps: Vec<String>,
  requests:   Vec<i64>,
  bytes:      Vec<i64>,
}

async fn cache_traffic_timeseries(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path(name): Path<String>,
  Query(query): Query<TimeseriesQuery>,
) -> Result<Json<TrafficSeries>, ApiError> {
  let Some(cache) = cache_overview::resolve_cache_ref(&state, &name).await?
  else {
    return Err(cache_not_found(&name));
  };
  let (granularity, points) = query.resolved();
  let series = circus_common::repo::cache_traffic::traffic_timeseries(
    &state.pool,
    &cache.name,
    granularity,
    points,
  )
  .await?;
  Ok(Json(TrafficSeries {
    timestamps: series.iter().map(|p| p.bucket_time.to_rfc3339()).collect(),
    requests:   series.iter().map(|p| p.requests).collect(),
    bytes:      series.iter().map(|p| p.bytes).collect(),
  }))
}

#[derive(Debug, Deserialize)]
struct NarsQuery {
  #[serde(default)]
  hash:    Option<String>,
  #[serde(default)]
  package: Option<String>,
  #[serde(default)]
  offset:  Option<i64>,
  #[serde(default)]
  limit:   Option<i64>,
}

#[derive(Debug, Serialize)]
struct NarExtremes {
  last_uploaded:  Option<chrono::DateTime<chrono::Utc>>,
  oldest_fetched: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Serialize)]
struct NarsResponse {
  items:    Vec<circus_common::repo::narinfo_cache::NarListItem>,
  total:    i64,
  summary:  circus_common::repo::narinfo_cache::CacheStorageSummary,
  extremes: NarExtremes,
}

/// Filtered, paginated NAR inventory for one cache. `hash` matches a store-path
/// hash prefix; `package` matches the post-hash name substring.
async fn list_cache_nars(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path(name): Path<String>,
  Query(query): Query<NarsQuery>,
) -> Result<Json<NarsResponse>, ApiError> {
  let Some(cache) = cache_overview::resolve_cache_ref(&state, &name).await?
  else {
    return Err(cache_not_found(&name));
  };
  let normalize = |value: Option<String>| {
    value.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty())
  };
  let hash = normalize(query.hash);
  let package = normalize(query.package);
  let limit = query.limit.unwrap_or(50).clamp(1, 500);
  let offset = query.offset.unwrap_or(0).max(0);

  let items = circus_common::repo::narinfo_cache::list_filtered(
    &state.pool,
    cache.scope,
    hash.as_deref(),
    package.as_deref(),
    limit,
    offset,
  )
  .await?;
  let total = circus_common::repo::narinfo_cache::count_filtered(
    &state.pool,
    cache.scope,
    hash.as_deref(),
    package.as_deref(),
  )
  .await?;
  let summary = circus_common::repo::narinfo_cache::storage_summary(
    &state.pool,
    cache.scope,
  )
  .await?;
  let (last_uploaded, oldest_fetched) =
    circus_common::repo::narinfo_cache::storage_extremes(
      &state.pool,
      cache.scope,
    )
    .await?;

  Ok(Json(NarsResponse {
    items,
    total,
    summary,
    extremes: NarExtremes {
      last_uploaded,
      oldest_fetched,
    },
  }))
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/admin/caches", get(list_caches))
    .route("/admin/caches/{name}", get(get_cache))
    .route(
      "/admin/caches/{name}/storage-timeseries",
      get(cache_storage_timeseries),
    )
    .route(
      "/admin/caches/{name}/traffic-timeseries",
      get(cache_traffic_timeseries),
    )
    .route("/admin/caches/{name}/nars", get(list_cache_nars))
    .route("/admin/builders", get(list_builders).post(create_builder))
    .route(
      "/admin/builders/{id}",
      get(get_builder).put(update_builder).delete(delete_builder),
    )
    .route("/admin/builders/sessions", get(list_builder_sessions))
    .route(
      "/admin/builders/sessions/connected",
      get(list_connected_builder_sessions),
    )
    .route(
      "/admin/builders/sessions/{machine_id}",
      get(get_builder_session),
    )
    .route("/admin/system", get(system_status))
    .route("/admin/notification-tasks", get(list_notification_tasks))
    .route(
      "/admin/notification-tasks/{id}/retry",
      post(retry_notification_task),
    )
    .route(
      "/admin/pinned-build-products",
      get(list_pinned_build_products),
    )
    .route("/admin/pinned-builds/{id}/unpin", post(unpin_build))
    .route(
      "/admin/config",
      get(get_config_file).put(update_config_file),
    )
    .route("/admin/audit-log", get(list_audit_log))
}
