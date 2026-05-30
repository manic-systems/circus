use axum::{
  Json,
  Router,
  extract::{Path, Query, State},
  routing::{get, post},
};
use circus_common::{
  Validate,
  audit::AuditEntry,
  models::{
    CreateRemoteBuilder,
    NotificationTask,
    RemoteBuilder,
    SystemStatus,
    UpdateRemoteBuilder,
  },
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{auth_middleware::RequireAdmin, error::ApiError, state::AppState};

fn config_file_path() -> std::path::PathBuf {
  std::env::var_os("CIRCUS_CONFIG_FILE").map_or_else(
    || std::path::PathBuf::from("circus.toml"),
    std::path::PathBuf::from,
  )
}

async fn list_builders(
  State(state): State<AppState>,
) -> Result<Json<Vec<RemoteBuilder>>, ApiError> {
  let builders = circus_common::repo::remote_builders::list(&state.pool)
    .await
    .map_err(ApiError)?;
  Ok(Json(builders))
}

/// All builder sessions known to the cluster, connected or not. Backed by
/// the `builder_sessions` table that the queue-runner upserts on register
/// and on heartbeat.
async fn list_builder_sessions(
  State(state): State<AppState>,
) -> Result<
  Json<Vec<circus_common::repo::builder_sessions::BuilderSession>>,
  ApiError,
> {
  let sessions = circus_common::repo::builder_sessions::list(&state.pool)
    .await
    .map_err(ApiError)?;
  Ok(Json(sessions))
}

/// Currently-connected agents only. The shape of the rows is the same as
/// [`list_builder_sessions`]; this endpoint matches the dashboard's
/// "live agents" panel.
async fn list_connected_builder_sessions(
  State(state): State<AppState>,
) -> Result<
  Json<Vec<circus_common::repo::builder_sessions::BuilderSession>>,
  ApiError,
> {
  let sessions =
    circus_common::repo::builder_sessions::list_connected(&state.pool)
      .await
      .map_err(ApiError)?;
  Ok(Json(sessions))
}

async fn get_builder_session(
  State(state): State<AppState>,
  Path(machine_id): Path<Uuid>,
) -> Result<Json<circus_common::repo::builder_sessions::BuilderSession>, ApiError>
{
  let session =
    circus_common::repo::builder_sessions::get(&state.pool, machine_id)
      .await
      .map_err(ApiError)?;
  Ok(Json(session))
}

async fn get_builder(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<RemoteBuilder>, ApiError> {
  let builder = circus_common::repo::remote_builders::get(&state.pool, id)
    .await
    .map_err(ApiError)?;
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
    circus_common::repo::remote_builders::create(&state.pool, input)
      .await
      .map_err(ApiError)?;

  crate::audit::record_for_key(
    &state.pool,
    &auth.0,
    "BUILDER_CREATE",
    Some("builder"),
    Some(&builder.id.to_string()),
    serde_json::json!({ "name": builder.name, "ssh_uri": builder.ssh_uri }),
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
  let builder =
    circus_common::repo::remote_builders::update(&state.pool, id, input)
      .await
      .map_err(ApiError)?;

  crate::audit::record_for_key(
    &state.pool,
    &auth.0,
    "BUILDER_UPDATE",
    Some("builder"),
    Some(&builder.id.to_string()),
    serde_json::json!({ "name": builder.name, "enabled": builder.enabled }),
  )
  .await;

  Ok(Json(builder))
}

async fn delete_builder(
  auth: RequireAdmin,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
  circus_common::repo::remote_builders::delete(&state.pool, id)
    .await
    .map_err(ApiError)?;

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

  let projects: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects")
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError(circus_common::CiError::Database(e)))?;
  let jobsets: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobsets")
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError(circus_common::CiError::Database(e)))?;
  let evaluations: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM evaluations")
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError(circus_common::CiError::Database(e)))?;

  let build_stats = circus_common::repo::builds::get_stats(pool)
    .await
    .map_err(ApiError)?;
  let builders = circus_common::repo::remote_builders::count(pool)
    .await
    .map_err(ApiError)?;

  let channels: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM channels")
    .fetch_one(pool)
    .await
    .map_err(|e| ApiError(circus_common::CiError::Database(e)))?;

  Ok(Json(SystemStatus {
    projects_count:    projects.0,
    jobsets_count:     jobsets.0,
    evaluations_count: evaluations.0,
    builds_pending:    build_stats.pending_builds.unwrap_or(0),
    builds_running:    build_stats.running_builds.unwrap_or(0),
    builds_completed:  build_stats.completed_builds.unwrap_or(0),
    builds_failed:     build_stats.failed_builds.unwrap_or(0),
    remote_builders:   builders,
    channels_count:    channels.0,
  }))
}

async fn list_notification_tasks(
  _auth: RequireAdmin,
  State(state): State<AppState>,
) -> Result<Json<Vec<NotificationTask>>, ApiError> {
  let tasks =
    circus_common::repo::notification_tasks::list_recent(&state.pool, 100)
      .await
      .map_err(ApiError)?;
  Ok(Json(tasks))
}

async fn retry_notification_task(
  auth: RequireAdmin,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<NotificationTask>, ApiError> {
  let task =
    circus_common::repo::notification_tasks::requeue_failed(&state.pool, id)
      .await
      .map_err(ApiError)?;

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

#[derive(Debug, Serialize)]
struct ConfigFileResponse {
  path:             String,
  contents:         String,
  requires_restart: bool,
}

#[derive(Debug, Deserialize)]
struct UpdateConfigFile {
  contents: String,
}

async fn get_config_file(
  _auth: RequireAdmin,
) -> Result<Json<ConfigFileResponse>, ApiError> {
  let path = config_file_path();
  let contents = match tokio::fs::read_to_string(&path).await {
    Ok(contents) => contents,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
      toml::to_string_pretty(&circus_common::config::Config::default())
        .map_err(|e| {
          ApiError(circus_common::CiError::Internal(format!(
            "Failed to render default configuration: {e}"
          )))
        })?
    },
    Err(e) => return Err(ApiError(circus_common::CiError::Io(e))),
  };

  Ok(Json(ConfigFileResponse {
    path: path.display().to_string(),
    contents,
    requires_restart: true,
  }))
}

async fn update_config_file(
  auth: RequireAdmin,
  State(state): State<AppState>,
  Json(input): Json<UpdateConfigFile>,
) -> Result<Json<ConfigFileResponse>, ApiError> {
  let parsed: circus_common::config::Config = toml::from_str(&input.contents)
    .map_err(|e| {
    ApiError(circus_common::CiError::Validation(format!(
      "Invalid TOML configuration: {e}"
    )))
  })?;
  parsed
    .validate()
    .map_err(|e| ApiError(circus_common::CiError::Validation(e.to_string())))?;

  let path = config_file_path();
  let tmp_path = path.with_extension("toml.tmp");
  tokio::fs::write(&tmp_path, &input.contents)
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
      "bytes":  input.contents.len(),
    }),
  )
  .await;

  Ok(Json(ConfigFileResponse {
    path:             path.display().to_string(),
    contents:         input.contents,
    requires_restart: true,
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

  let items = circus_common::audit::list(&state.pool, limit, offset)
    .await
    .map_err(ApiError)?;
  let total = circus_common::audit::count(&state.pool)
    .await
    .map_err(ApiError)?;

  Ok(Json(AuditLogPage {
    items,
    total,
    limit,
    offset,
  }))
}

pub fn router() -> Router<AppState> {
  Router::new()
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
      "/admin/config",
      get(get_config_file).put(update_config_file),
    )
    .route("/admin/audit-log", get(list_audit_log))
}
