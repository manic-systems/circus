use axum::{
  Json,
  Router,
  extract::{Path, State},
  routing::{delete, get},
};
use circus_common::models::{CreateNewsItem, NewsItem};
use serde::Deserialize;
use uuid::Uuid;

use crate::{auth_middleware::RequireAdmin, error::ApiError, state::AppState};

#[derive(Deserialize)]
struct PageParams {
  limit:  Option<i64>,
  offset: Option<i64>,
}

async fn list_news(
  State(state): State<AppState>,
  axum::extract::Query(params): axum::extract::Query<PageParams>,
) -> Result<Json<Vec<NewsItem>>, ApiError> {
  let limit = params.limit.unwrap_or(20).clamp(1, 100);
  let offset = params.offset.unwrap_or(0).max(0);
  let items =
    circus_common::repo::news::list(&state.pool, limit, offset).await?;
  Ok(Json(items))
}

#[derive(Deserialize)]
struct CreateNewsRequest {
  title:   String,
  content: String,
}

async fn create_news(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Json(body): Json<CreateNewsRequest>,
) -> Result<Json<NewsItem>, ApiError> {
  if body.title.trim().is_empty() {
    return Err(ApiError(circus_common::CiError::Validation(
      "Title must not be empty".to_string(),
    )));
  }
  let item = circus_common::repo::news::create(&state.pool, CreateNewsItem {
    title:      body.title.trim().to_string(),
    content:    body.content,
    created_by: None,
  })
  .await?;
  Ok(Json(item))
}

async fn delete_news(
  _auth: RequireAdmin,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, ApiError> {
  circus_common::repo::news::delete(&state.pool, id).await?;
  Ok(Json(serde_json::json!({"deleted": true})))
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/news", get(list_news).post(create_news))
    .route("/news/{id}", delete(delete_news))
}
