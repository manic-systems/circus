use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  error::{CiError, Result},
  models::{CreateNewsItem, NewsItem},
};

/// Create a news/announcement item.
///
/// # Errors
///
/// Returns error if database insert fails.
pub async fn create(pool: &PgPool, input: CreateNewsItem) -> Result<NewsItem> {
  Ok(
    sqlx::query_as::<_, NewsItem>(
      "INSERT INTO news (title, content, created_by) VALUES ($1, $2, $3) \
       RETURNING *",
    )
    .bind(&input.title)
    .bind(&input.content)
    .bind(input.created_by)
    .fetch_one(pool)
    .await?,
  )
}

/// List news items, most recent first.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list(
  pool: &PgPool,
  limit: i64,
  offset: i64,
) -> Result<Vec<NewsItem>> {
  Ok(
    sqlx::query_as::<_, NewsItem>(
      "SELECT * FROM news ORDER BY created_at DESC LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?,
  )
}

/// Count total news items.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count(pool: &PgPool) -> Result<i64> {
  let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM news")
    .fetch_one(pool)
    .await?;
  Ok(row.0)
}

/// Delete a news item by ID.
///
/// # Errors
///
/// Returns error if database delete fails or item not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let result = sqlx::query("DELETE FROM news WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await?;
  if result.rows_affected() == 0 {
    return Err(CiError::NotFound(format!("News item {id} not found")));
  }
  Ok(())
}
