use circus_codegen::queries::news as q;
use uuid::Uuid;

use crate::{
  db::PgPool,
  error::{CiError, Result},
  models::{CreateNewsItem, NewsItem},
};

impl From<q::NewsRow> for NewsItem {
  fn from(r: q::NewsRow) -> Self {
    Self {
      id:         r.id,
      title:      r.title,
      content:    r.content,
      created_by: r.created_by,
      created_at: r.created_at,
    }
  }
}

/// Create a news/announcement item.
///
/// # Errors
///
/// Returns error if database insert fails.
pub async fn create(pool: &PgPool, input: CreateNewsItem) -> Result<NewsItem> {
  let client = pool.get().await?;
  Ok(
    q::create()
      .bind(&client, &input.title, &input.content, &input.created_by)
      .one()
      .await
      .map(NewsItem::from)?,
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
  let client = pool.get().await?;
  let rows = q::list().bind(&client, &limit, &offset).all().await?;
  Ok(rows.into_iter().map(NewsItem::from).collect())
}

/// Count total news items.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count(pool: &PgPool) -> Result<i64> {
  let client = pool.get().await?;
  Ok(q::count().bind(&client).one().await?)
}

/// Delete a news item by ID.
///
/// # Errors
///
/// Returns error if database delete fails or item not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let client = pool.get().await?;
  let affected = q::delete().bind(&client, &id).await?;
  if affected == 0 {
    return Err(CiError::NotFound(format!("News item {id} not found")));
  }
  Ok(())
}
