use axum::{
  Json,
  Router,
  extract::{Path, Query, State},
  http::Extensions,
  routing::{delete, get, post},
};
use chrono::{DateTime, Utc};
use circus_common::{
  CreateJobset,
  CreateProject,
  Jobset,
  PaginatedResponse,
  PaginationParams,
  Project,
  UpdateProject,
  Validate,
  WebhookConfig,
  models::{CreateWebhookConfig, ForgeType},
  nix,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
  auth_middleware::RequireAdmin,
  error::ApiError,
  permissions::{self, Permission},
  state::AppState,
};

#[derive(Debug, Serialize)]
struct WebhookConfigResponse {
  id:         Uuid,
  project_id: Uuid,
  forge_type: String,
  enabled:    bool,
  created_at: DateTime<Utc>,
}

impl From<WebhookConfig> for WebhookConfigResponse {
  fn from(c: WebhookConfig) -> Self {
    Self {
      id:         c.id,
      project_id: c.project_id,
      forge_type: c.forge_type.to_string(),
      enabled:    c.enabled,
      created_at: c.created_at,
    }
  }
}

async fn list_projects(
  State(state): State<AppState>,
  Query(pagination): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<Project>>, ApiError> {
  let limit = pagination.limit();
  let offset = pagination.offset();
  let items = circus_common::repo::projects::list(&state.pool, limit, offset)
    .await
    .map_err(ApiError)?;
  let total = circus_common::repo::projects::count(&state.pool)
    .await
    .map_err(ApiError)?;
  Ok(Json(PaginatedResponse {
    items,
    total,
    limit,
    offset,
  }))
}

async fn create_project(
  extensions: Extensions,
  State(state): State<AppState>,
  Json(input): Json<CreateProject>,
) -> Result<Json<Project>, ApiError> {
  permissions::require_api(&extensions, Permission::CreateProjects)?;
  input
    .validate()
    .map_err(|msg| ApiError(circus_common::CiError::Validation(msg)))?;
  circus_common::validate::validate_url_scheme(
    &input.repository_url,
    &state.config.server.allowed_url_schemes,
  )
  .map_err(|msg| ApiError(circus_common::CiError::Validation(msg)))?;
  let project = circus_common::repo::projects::create(&state.pool, input)
    .await
    .map_err(ApiError)?;
  Ok(Json(project))
}

async fn get_project(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Project>, ApiError> {
  let project = circus_common::repo::projects::get(&state.pool, id)
    .await
    .map_err(ApiError)?;
  Ok(Json(project))
}

async fn update_project(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  Json(input): Json<UpdateProject>,
) -> Result<Json<Project>, ApiError> {
  input
    .validate()
    .map_err(|msg| ApiError(circus_common::CiError::Validation(msg)))?;
  if let Some(ref url) = input.repository_url {
    circus_common::validate::validate_url_scheme(
      url,
      &state.config.server.allowed_url_schemes,
    )
    .map_err(|msg| ApiError(circus_common::CiError::Validation(msg)))?;
  }
  let project = circus_common::repo::projects::update(&state.pool, id, input)
    .await
    .map_err(ApiError)?;
  Ok(Json(project))
}

async fn delete_project(
  auth: RequireAdmin,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
  // Capture the name before deletion so the audit row remains readable.
  let project_name = circus_common::repo::projects::get(&state.pool, id)
    .await
    .ok()
    .map(|p| p.name);

  circus_common::repo::projects::delete(&state.pool, id)
    .await
    .map_err(ApiError)?;

  crate::audit::record_for_key(
    &state.pool,
    &auth.0,
    "PROJECT_DELETE",
    Some("project"),
    Some(&id.to_string()),
    serde_json::json!({ "name": project_name }),
  )
  .await;

  Ok(Json(serde_json::json!({ "deleted": true })))
}

async fn list_project_jobsets(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  Query(pagination): Query<PaginationParams>,
) -> Result<Json<PaginatedResponse<Jobset>>, ApiError> {
  let limit = pagination.limit();
  let offset = pagination.offset();
  let items = circus_common::repo::jobsets::list_for_project(
    &state.pool,
    id,
    limit,
    offset,
  )
  .await
  .map_err(ApiError)?;
  let total = circus_common::repo::jobsets::count_for_project(&state.pool, id)
    .await
    .map_err(ApiError)?;
  Ok(Json(PaginatedResponse {
    items,
    total,
    limit,
    offset,
  }))
}

#[derive(Debug, Deserialize)]
struct CreateJobsetBody {
  name:              String,
  nix_expression:    String,
  enabled:           Option<bool>,
  flake_mode:        Option<bool>,
  check_interval:    Option<i32>,
  trigger_mode:      Option<circus_common::models::JobsetTriggerMode>,
  branch:            Option<String>,
  scheduling_shares: Option<i32>,
  state:             Option<circus_common::models::JobsetState>,
}

async fn create_project_jobset(
  extensions: Extensions,
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  Json(body): Json<CreateJobsetBody>,
) -> Result<Json<Jobset>, ApiError> {
  permissions::require_api(&extensions, Permission::CreateProjects)?;
  let input = CreateJobset {
    project_id,
    name: body.name,
    nix_expression: body.nix_expression,
    enabled: body.enabled,
    flake_mode: body.flake_mode,
    check_interval: body.check_interval,
    trigger_mode: body.trigger_mode,
    branch: body.branch,
    scheduling_shares: body.scheduling_shares,
    state: body.state,
    keep_nr: None,
  };
  input
    .validate()
    .map_err(|msg| ApiError(circus_common::CiError::Validation(msg)))?;
  let jobset = circus_common::repo::jobsets::create(&state.pool, input)
    .await
    .map_err(ApiError)?;
  Ok(Json(jobset))
}

#[derive(Debug, Deserialize)]
struct ProbeRequest {
  repository_url: String,
  revision:       Option<String>,
}

async fn probe_repository(
  extensions: Extensions,
  State(state): State<AppState>,
  Json(body): Json<ProbeRequest>,
) -> Result<Json<nix::FlakeProbeResult>, ApiError> {
  permissions::require_api(&extensions, Permission::CreateProjects)?;
  circus_common::validate::validate_url_scheme(
    &body.repository_url,
    &state.config.server.allowed_url_schemes,
  )
  .map_err(|msg| ApiError(circus_common::CiError::Validation(msg)))?;
  let result =
    nix::probe::probe_flake(&body.repository_url, body.revision.as_deref())
      .await
      .map_err(ApiError)?;
  Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct SetupJobsetInput {
  name:           String,
  nix_expression: String,
}

#[derive(Debug, Deserialize)]
struct SetupProjectRequest {
  repository_url: String,
  name:           String,
  description:    Option<String>,
  jobsets:        Vec<SetupJobsetInput>,
}

#[derive(serde::Serialize)]
struct SetupProjectResponse {
  project: Project,
  jobsets: Vec<Jobset>,
}

async fn setup_project(
  extensions: Extensions,
  State(state): State<AppState>,
  Json(body): Json<SetupProjectRequest>,
) -> Result<Json<SetupProjectResponse>, ApiError> {
  permissions::require_api(&extensions, Permission::CreateProjects)?;

  let create_project = CreateProject {
    name:           body.name,
    repository_url: body.repository_url,
    description:    body.description,
  };
  create_project
    .validate()
    .map_err(|msg| ApiError(circus_common::CiError::Validation(msg)))?;
  circus_common::validate::validate_url_scheme(
    &create_project.repository_url,
    &state.config.server.allowed_url_schemes,
  )
  .map_err(|msg| ApiError(circus_common::CiError::Validation(msg)))?;

  let project =
    circus_common::repo::projects::create(&state.pool, create_project)
      .await
      .map_err(ApiError)?;

  let mut jobsets = Vec::new();
  for js_input in body.jobsets {
    let input = CreateJobset {
      project_id:        project.id,
      name:              js_input.name,
      nix_expression:    js_input.nix_expression,
      enabled:           Some(true),
      flake_mode:        Some(true),
      check_interval:    None,
      trigger_mode:      None,
      branch:            None,
      scheduling_shares: None,
      state:             None,
      keep_nr:           None,
    };
    input
      .validate()
      .map_err(|msg| ApiError(circus_common::CiError::Validation(msg)))?;
    let jobset = circus_common::repo::jobsets::create(&state.pool, input)
      .await
      .map_err(ApiError)?;
    jobsets.push(jobset);
  }

  Ok(Json(SetupProjectResponse { project, jobsets }))
}

// Webhook configuration routes

#[derive(Debug, Deserialize)]
struct CreateWebhookBody {
  forge_type: ForgeType,
  secret:     String,
}

async fn list_project_webhooks(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Vec<WebhookConfigResponse>>, ApiError> {
  let configs =
    circus_common::repo::webhook_configs::list_for_project(&state.pool, id)
      .await
      .map_err(ApiError)?;
  Ok(Json(
    configs
      .into_iter()
      .map(WebhookConfigResponse::from)
      .collect(),
  ))
}

async fn create_project_webhook(
  extensions: Extensions,
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  Json(body): Json<CreateWebhookBody>,
) -> Result<Json<WebhookConfigResponse>, ApiError> {
  permissions::require_api(&extensions, Permission::CreateProjects)?;

  if body.secret.trim().is_empty() {
    return Err(ApiError(circus_common::CiError::Validation(
      "Webhook secret is required".into(),
    )));
  }

  let input = CreateWebhookConfig {
    project_id,
    forge_type: body.forge_type,
    secret: Some(body.secret.clone()),
  };

  let config = circus_common::repo::webhook_configs::create(
    &state.pool,
    input,
    Some(body.secret.as_str()),
    state.config.server.webhook_secret_encryption_key.as_deref(),
  )
  .await
  .map_err(ApiError)?;

  Ok(Json(WebhookConfigResponse::from(config)))
}

#[derive(Deserialize)]
struct WebhookPathParams {
  id:         Uuid,
  webhook_id: Uuid,
}

async fn delete_project_webhook(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path(params): Path<WebhookPathParams>,
) -> Result<Json<serde_json::Value>, ApiError> {
  // Verify the webhook belongs to the project
  let config =
    circus_common::repo::webhook_configs::get(&state.pool, params.webhook_id)
      .await
      .map_err(ApiError)?;

  if config.project_id != params.id {
    return Err(ApiError(circus_common::CiError::NotFound(
      "Webhook not found for this project".to_string(),
    )));
  }

  circus_common::repo::webhook_configs::delete(&state.pool, params.webhook_id)
    .await
    .map_err(ApiError)?;

  Ok(Json(serde_json::json!({ "deleted": true })))
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/projects", get(list_projects).post(create_project))
    .route("/projects/probe", post(probe_repository))
    .route("/projects/setup", post(setup_project))
    .route(
      "/projects/{id}",
      get(get_project).put(update_project).delete(delete_project),
    )
    .route(
      "/projects/{id}/jobsets",
      get(list_project_jobsets).post(create_project_jobset),
    )
    .route(
      "/projects/{id}/webhooks",
      get(list_project_webhooks).post(create_project_webhook),
    )
    .route(
      "/projects/{id}/webhooks/{webhook_id}",
      delete(delete_project_webhook),
    )
}
