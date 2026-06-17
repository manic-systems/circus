use axum::{Json, Router, extract::State, routing::get};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use circus_common::{repo, roles::GlobalRole};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{auth_middleware::RequireAdmin, error::ApiError, state::AppState};

#[derive(Debug, Deserialize)]
pub struct CreateApiKeyRequest {
  pub name: String,
  pub role: Option<GlobalRole>,
}

#[derive(Debug, Serialize)]
pub struct CreateApiKeyResponse {
  pub id:   Uuid,
  pub name: String,
  pub key:  String,
  pub role: String,
}

#[derive(Debug, Serialize)]
pub struct ApiKeyInfo {
  pub id:           Uuid,
  pub name:         String,
  pub role:         String,
  pub created_at:   chrono::DateTime<chrono::Utc>,
  pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[must_use]
pub fn hash_api_key(key: &str) -> String {
  let mut hasher = Sha256::new();
  hasher.update(key.as_bytes());
  hex::encode(hasher.finalize())
}

async fn create_api_key(
  auth: RequireAdmin,
  State(state): State<AppState>,
  Json(input): Json<CreateApiKeyRequest>,
) -> Result<Json<CreateApiKeyResponse>, ApiError> {
  let role = input.role.unwrap_or(GlobalRole::ReadOnly);

  let mut bytes = [0u8; 32];
  ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes)
    .map_err(|_| {
      ApiError(circus_common::CiError::Internal(
        "Failed to generate random API key".into(),
      ))
    })?;
  let key = format!("circus_{}", URL_SAFE_NO_PAD.encode(bytes));
  let key_hash = hash_api_key(&key);

  let api_key =
    repo::api_keys::create(&state.pool, &input.name, &key_hash, role).await?;

  crate::audit::record_for_key(
    &state.pool,
    &auth.0,
    "API_KEY_CREATE",
    Some("api_key"),
    Some(&api_key.id.to_string()),
    serde_json::json!({ "name": api_key.name, "role": api_key.role }),
  )
  .await;

  Ok(Json(CreateApiKeyResponse {
    id: api_key.id,
    name: api_key.name,
    key, // Only returned once at creation time
    role: api_key.role.to_string(),
  }))
}

async fn list_api_keys(
  _auth: RequireAdmin,
  State(state): State<AppState>,
) -> Result<Json<Vec<ApiKeyInfo>>, ApiError> {
  let keys = repo::api_keys::list(&state.pool).await?;

  let infos: Vec<ApiKeyInfo> = keys
    .into_iter()
    .map(|k| {
      ApiKeyInfo {
        id:           k.id,
        name:         k.name,
        role:         k.role.to_string(),
        created_at:   k.created_at,
        last_used_at: k.last_used_at,
      }
    })
    .collect();

  Ok(Json(infos))
}

async fn delete_api_key(
  auth: RequireAdmin,
  State(state): State<AppState>,
  axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
  repo::api_keys::delete(&state.pool, id).await?;

  crate::audit::record_for_key(
    &state.pool,
    &auth.0,
    "API_KEY_DELETE",
    Some("api_key"),
    Some(&id.to_string()),
    serde_json::Value::Null,
  )
  .await;

  Ok(Json(serde_json::json!({ "deleted": true })))
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/api-keys", get(list_api_keys).post(create_api_key))
    .route("/api-keys/{id}", axum::routing::delete(delete_api_key))
}
