use axum::{
  Json,
  Router,
  extract::{Path, Query, State},
  http::Extensions,
  routing::{delete, get, post},
};
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
  models::CreateWebhookConfig,
  nix_probe,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
  auth_middleware::{RequireAdmin, RequireRoles},
  error::ApiError,
  state::AppState,
};

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
  RequireRoles::check(&extensions, &["create-projects"]).map_err(|s| {
    ApiError(if s == axum::http::StatusCode::FORBIDDEN {
      circus_common::CiError::Forbidden("Insufficient permissions".to_string())
    } else {
      circus_common::CiError::Unauthorized(
        "Authentication required".to_string(),
      )
    })
  })?;
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
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
  circus_common::repo::projects::delete(&state.pool, id)
    .await
    .map_err(ApiError)?;
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
  RequireRoles::check(&extensions, &["create-projects"]).map_err(|s| {
    ApiError(if s == axum::http::StatusCode::FORBIDDEN {
      circus_common::CiError::Forbidden("Insufficient permissions".to_string())
    } else {
      circus_common::CiError::Unauthorized(
        "Authentication required".to_string(),
      )
    })
  })?;
  let input = CreateJobset {
    project_id,
    name: body.name,
    nix_expression: body.nix_expression,
    enabled: body.enabled,
    flake_mode: body.flake_mode,
    check_interval: body.check_interval,
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
  _extensions: Extensions,
  Json(body): Json<ProbeRequest>,
) -> Result<Json<nix_probe::FlakeProbeResult>, ApiError> {
  let result =
    nix_probe::probe_flake(&body.repository_url, body.revision.as_deref())
      .await
      .map_err(ApiError)?;
  Ok(Json(result))
}

#[derive(Debug, Deserialize)]
struct SetupJobsetInput {
  name:           String,
  nix_expression: String,
  #[allow(dead_code)]
  description:    Option<String>,
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
  RequireRoles::check(&extensions, &["create-projects"]).map_err(|s| {
    ApiError(if s == axum::http::StatusCode::FORBIDDEN {
      circus_common::CiError::Forbidden("Insufficient permissions".to_string())
    } else {
      circus_common::CiError::Unauthorized(
        "Authentication required".to_string(),
      )
    })
  })?;

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
  forge_type: String,
  secret:     Option<String>,
}

async fn list_project_webhooks(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<Vec<WebhookConfig>>, ApiError> {
  let configs =
    circus_common::repo::webhook_configs::list_for_project(&state.pool, id)
      .await
      .map_err(ApiError)?;
  Ok(Json(configs))
}

async fn create_project_webhook(
  extensions: Extensions,
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  Json(body): Json<CreateWebhookBody>,
) -> Result<Json<WebhookConfig>, ApiError> {
  RequireRoles::check(&extensions, &["create-projects"]).map_err(|s| {
    ApiError(if s == axum::http::StatusCode::FORBIDDEN {
      circus_common::CiError::Forbidden("Insufficient permissions".to_string())
    } else {
      circus_common::CiError::Unauthorized(
        "Authentication required".to_string(),
      )
    })
  })?;

  // Validate forge type
  let valid_forges = ["github", "gitlab", "gitea", "forgejo"];
  if !valid_forges.contains(&body.forge_type.as_str()) {
    return Err(ApiError(circus_common::CiError::Validation(format!(
      "Invalid forge_type '{}'. Must be one of: {}",
      body.forge_type,
      valid_forges.join(", ")
    ))));
  }

  let input = CreateWebhookConfig {
    project_id,
    forge_type: body.forge_type,
    secret: body.secret.clone(),
  };

  // For webhook configs, we store the secret directly (used for token
  // comparison) GitHub/Gitea use HMAC verification, GitLab uses direct token
  // comparison
  let config = circus_common::repo::webhook_configs::create(
    &state.pool,
    input,
    body.secret.as_deref(),
  )
  .await
  .map_err(ApiError)?;

  Ok(Json(config))
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
