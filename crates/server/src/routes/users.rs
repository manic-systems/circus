//! User management API routes

use axum::{
  Json,
  Router,
  extract::{Path, Query, State},
  http::StatusCode,
  routing::get,
};
use circus_common::{
  models::{CreateStarredJob, CreateUser, PaginationParams, UpdateUser, User},
  repo::{self},
  roles::GlobalRole,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{auth_middleware::RequireAdmin, error::ApiError, state::AppState};

// Request/response DTOs

#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
  pub username:  String,
  pub email:     String,
  pub full_name: Option<String>,
  pub password:  String,
  pub role:      Option<GlobalRole>,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
  pub id:               Uuid,
  pub username:         String,
  pub email:            String,
  pub full_name:        Option<String>,
  pub user_type:        String,
  pub role:             String,
  pub enabled:          bool,
  pub email_verified:   bool,
  pub public_dashboard: bool,
  pub created_at:       chrono::DateTime<chrono::Utc>,
  pub updated_at:       chrono::DateTime<chrono::Utc>,
  pub last_login_at:    Option<chrono::DateTime<chrono::Utc>>,
}

impl From<User> for UserResponse {
  fn from(u: User) -> Self {
    Self {
      id:               u.id,
      username:         u.username,
      email:            u.email,
      full_name:        u.full_name,
      user_type:        format!("{:?}", u.user_type).to_lowercase(),
      role:             u.role.to_string(),
      enabled:          u.enabled,
      email_verified:   u.email_verified,
      public_dashboard: u.public_dashboard,
      created_at:       u.created_at,
      updated_at:       u.updated_at,
      last_login_at:    u.last_login_at,
    }
  }
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
  pub email:            Option<String>,
  pub full_name:        Option<String>,
  pub password:         Option<String>,
  pub role:             Option<GlobalRole>,
  pub enabled:          Option<bool>,
  pub public_dashboard: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
  pub username: String,
  pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
  pub user: UserResponse,
}

#[derive(Debug, Serialize)]
pub struct StarredJobResponse {
  pub id:         Uuid,
  pub project_id: Uuid,
  pub jobset_id:  Option<Uuid>,
  pub job_name:   String,
  pub created_at: chrono::DateTime<chrono::Utc>,
}

// Admin user management handlers

async fn list_users(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Query(params): Query<PaginationParams>,
) -> Result<Json<Vec<UserResponse>>, ApiError> {
  let users =
    repo::users::list(&state.pool, params.limit(), params.offset()).await?;
  Ok(Json(users.into_iter().map(UserResponse::from).collect()))
}

async fn create_user(
  auth: RequireAdmin,
  State(state): State<AppState>,
  Json(req): Json<CreateUserRequest>,
) -> Result<Json<UserResponse>, ApiError> {
  let data = CreateUser {
    username:  req.username,
    email:     req.email,
    full_name: req.full_name,
    password:  req.password,
    role:      req.role,
  };

  let user =
    repo::users::create(&state.pool, &data, state.email_regex.as_deref())
      .await?;

  crate::audit::record_for_key(
    &state.pool,
    &auth.0,
    "USER_CREATE",
    Some("user"),
    Some(&user.id.to_string()),
    serde_json::json!({ "username": user.username, "role": user.role }),
  )
  .await;

  Ok(Json(UserResponse::from(user)))
}

async fn get_user(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<UserResponse>, ApiError> {
  let user = repo::users::get(&state.pool, id).await?;
  Ok(Json(UserResponse::from(user)))
}

async fn update_user(
  auth: RequireAdmin,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  Json(req): Json<UpdateUserRequest>,
) -> Result<Json<UserResponse>, ApiError> {
  // Snapshot what is being changed (without leaking the password itself)
  // so the audit row tells a reviewer which fields were touched.
  let mut changed: Vec<&'static str> = Vec::new();
  if req.email.is_some() {
    changed.push("email");
  }
  if req.full_name.is_some() {
    changed.push("full_name");
  }
  if req.password.is_some() {
    changed.push("password");
  }
  if req.role.is_some() {
    changed.push("role");
  }
  if req.enabled.is_some() {
    changed.push("enabled");
  }
  if req.public_dashboard.is_some() {
    changed.push("public_dashboard");
  }

  let data = UpdateUser {
    email:            req.email,
    full_name:        req.full_name,
    password:         req.password,
    role:             req.role,
    enabled:          req.enabled,
    public_dashboard: req.public_dashboard,
  };

  let user =
    repo::users::update(&state.pool, id, &data, state.email_regex.as_deref())
      .await?;

  crate::audit::record_for_key(
    &state.pool,
    &auth.0,
    "USER_UPDATE",
    Some("user"),
    Some(&user.id.to_string()),
    serde_json::json!({
      "fields_changed": changed,
      "new_role":       req.role,
    }),
  )
  .await;

  Ok(Json(UserResponse::from(user)))
}

async fn delete_user(
  auth: RequireAdmin,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
  repo::users::delete(&state.pool, id).await?;

  crate::audit::record_for_key(
    &state.pool,
    &auth.0,
    "USER_DELETE",
    Some("user"),
    Some(&id.to_string()),
    serde_json::Value::Null,
  )
  .await;

  Ok(StatusCode::NO_CONTENT)
}

// Current user (self-service) handlers

async fn get_current_user(
  extensions: axum::http::Extensions,
) -> Result<Json<UserResponse>, ApiError> {
  // Try to get user from extensions first
  if let Some(user) = extensions.get::<User>() {
    return Ok(Json(UserResponse::from(user.clone())));
  }

  // Fall back to API key
  let api_key = extensions
    .get::<circus_common::models::ApiKey>()
    .cloned()
    .ok_or_else(|| {
      ApiError(circus_common::error::CiError::Unauthorized(
        "Not authenticated".to_string(),
      ))
    })?;

  // For API key auth, we don't have a user record yet
  // Return a synthetic user response based on the API key
  let synthetic_user = UserResponse {
    id:               api_key.id,
    username:         api_key.name.clone(),
    email:            String::new(),
    full_name:        None,
    user_type:        "api_key".to_string(),
    role:             api_key.role.to_string(),
    enabled:          true,
    email_verified:   true,
    public_dashboard: false,
    created_at:       api_key.created_at,
    updated_at:       api_key.created_at,
    last_login_at:    api_key.last_used_at,
  };

  Ok(Json(synthetic_user))
}

async fn update_current_user(
  State(state): State<AppState>,
  extensions: axum::http::Extensions,
  Json(req): Json<UpdateUserRequest>,
) -> Result<Json<UserResponse>, ApiError> {
  let user = extensions.get::<User>().cloned().ok_or_else(|| {
    ApiError(circus_common::error::CiError::Unauthorized(
      "User authentication required".to_string(),
    ))
  })?;

  if let Some(ref full_name) = req.full_name {
    repo::users::update_full_name(
      &state.pool,
      user.id,
      Some(full_name.as_str()),
    )
    .await?;
  }

  if let Some(ref email) = req.email {
    repo::users::update_email(
      &state.pool,
      user.id,
      email,
      state.email_regex.as_deref(),
    )
    .await?;
  }

  if let Some(public) = req.public_dashboard {
    repo::users::set_public_dashboard(&state.pool, user.id, public).await?;
  }

  let updated_user = repo::users::get(&state.pool, user.id).await?;
  Ok(Json(UserResponse::from(updated_user)))
}

async fn change_password(
  State(state): State<AppState>,
  extensions: axum::http::Extensions,
  Json(req): Json<ChangePasswordRequest>,
) -> Result<StatusCode, ApiError> {
  let user = extensions.get::<User>().cloned().ok_or_else(|| {
    ApiError(circus_common::error::CiError::Unauthorized(
      "User authentication required".to_string(),
    ))
  })?;

  // Verify current password (OAuth users don't have passwords)
  let hash = user.password_hash.ok_or_else(|| {
    ApiError(circus_common::error::CiError::Unauthorized(
      "OAuth user - use OAuth login".to_string(),
    ))
  })?;

  if !repo::users::verify_password(&req.current_password, &hash).map_err(
    |e| ApiError(circus_common::error::CiError::Internal(e.to_string())),
  )? {
    return Err(ApiError(circus_common::error::CiError::Unauthorized(
      "Current password is incorrect".to_string(),
    )));
  }

  repo::users::update_password(&state.pool, user.id, &req.new_password).await?;

  crate::audit::record_action(
    &state.pool,
    &extensions,
    "USER_PASSWORD_CHANGE",
    Some("user"),
    Some(&user.id.to_string()),
    serde_json::json!({ "self_service": true }),
  )
  .await;

  Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
  pub current_password: String,
  pub new_password:     String,
}

// Starred jobs handlers

async fn list_starred_jobs(
  State(state): State<AppState>,
  extensions: axum::http::Extensions,
  Query(params): Query<PaginationParams>,
) -> Result<Json<Vec<StarredJobResponse>>, ApiError> {
  let user = extensions.get::<User>().cloned().ok_or_else(|| {
    ApiError(circus_common::error::CiError::Unauthorized(
      "User authentication required".to_string(),
    ))
  })?;

  let jobs = repo::starred_jobs::list_for_user(
    &state.pool,
    user.id,
    params.limit(),
    params.offset(),
  )
  .await?;

  Ok(Json(
    jobs
      .into_iter()
      .map(|j| {
        StarredJobResponse {
          id:         j.id,
          project_id: j.project_id,
          jobset_id:  j.jobset_id,
          job_name:   j.job_name,
          created_at: j.created_at,
        }
      })
      .collect(),
  ))
}

async fn create_starred_job(
  State(state): State<AppState>,
  extensions: axum::http::Extensions,
  Json(req): Json<CreateStarredJobRequest>,
) -> Result<Json<StarredJobResponse>, ApiError> {
  let user = extensions.get::<User>().cloned().ok_or_else(|| {
    ApiError(circus_common::error::CiError::Unauthorized(
      "User authentication required".to_string(),
    ))
  })?;

  let data = CreateStarredJob {
    project_id: req.project_id,
    jobset_id:  req.jobset_id,
    job_name:   req.job_name,
  };

  let job = repo::starred_jobs::create(&state.pool, user.id, &data).await?;

  Ok(Json(StarredJobResponse {
    id:         job.id,
    project_id: job.project_id,
    jobset_id:  job.jobset_id,
    job_name:   job.job_name,
    created_at: job.created_at,
  }))
}

#[derive(Debug, Deserialize)]
pub struct CreateStarredJobRequest {
  pub project_id: Uuid,
  pub jobset_id:  Option<Uuid>,
  pub job_name:   String,
}

async fn delete_starred_job(
  State(state): State<AppState>,
  extensions: axum::http::Extensions,
  Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
  let _user = extensions.get::<User>().cloned().ok_or_else(|| {
    ApiError(circus_common::error::CiError::Unauthorized(
      "User authentication required".to_string(),
    ))
  })?;

  repo::starred_jobs::delete(&state.pool, id).await?;
  Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> Router<AppState> {
  Router::new()
    // User management (admin only)
    .route("/users", get(list_users).post(create_user))
    .route("/users/{id}", get(get_user).put(update_user).delete(delete_user))
    // Current user
    .route("/me", get(get_current_user).put(update_current_user))
    .route("/me/password", axum::routing::post(change_password))
    // Starred jobs
    .route("/me/starred-jobs", get(list_starred_jobs).post(create_starred_job))
    .route("/me/starred-jobs/{id}", axum::routing::delete(delete_starred_job))
}
