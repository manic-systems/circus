use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  config::DeclarativeChannel,
  error::{CiError, Result},
  models::{Channel, CreateChannel},
};

/// Create a release channel.
///
/// # Errors
///
/// Returns error if database insert fails or channel already exists.
pub async fn create(pool: &PgPool, input: CreateChannel) -> Result<Channel> {
  sqlx::query_as::<_, Channel>(
    "INSERT INTO channels (project_id, name, jobset_id) VALUES ($1, $2, $3) \
     RETURNING *",
  )
  .bind(input.project_id)
  .bind(&input.name)
  .bind(input.jobset_id)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict(format!(
          "Channel '{}' already exists for this project",
          input.name
        ))
      },
      _ => CiError::Database(e),
    }
  })
}

/// Get a channel by ID.
///
/// # Errors
///
/// Returns error if database query fails or channel not found.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<Channel> {
  sqlx::query_as::<_, Channel>("SELECT * FROM channels WHERE id = $1")
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| CiError::NotFound(format!("Channel {id} not found")))
}

/// List all channels for a project.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_for_project(
  pool: &PgPool,
  project_id: Uuid,
) -> Result<Vec<Channel>> {
  sqlx::query_as::<_, Channel>(
    "SELECT * FROM channels WHERE project_id = $1 ORDER BY name",
  )
  .bind(project_id)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

/// List all channels.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_all(pool: &PgPool) -> Result<Vec<Channel>> {
  sqlx::query_as::<_, Channel>("SELECT * FROM channels ORDER BY name")
    .fetch_all(pool)
    .await
    .map_err(CiError::Database)
}

/// Look up a channel by name. Names are unique within a project, but channel
/// manifest URLs are typically resolved by name only; the newest match wins
/// when multiple projects share the same channel name.
///
/// # Errors
///
/// Returns error if the database query fails or no channel matches.
pub async fn get_by_name(pool: &PgPool, name: &str) -> Result<Channel> {
  sqlx::query_as::<_, Channel>(
    "SELECT * FROM channels WHERE name = $1 ORDER BY created_at DESC, id DESC \
     LIMIT 1",
  )
  .bind(name)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| CiError::NotFound(format!("Channel '{name}' not found")))
}

/// Promote an evaluation to a channel (set it as the current evaluation).
///
/// # Errors
///
/// Returns error if database update fails or channel not found.
pub async fn promote(
  pool: &PgPool,
  channel_id: Uuid,
  evaluation_id: Uuid,
) -> Result<Channel> {
  sqlx::query_as::<_, Channel>(
    "UPDATE channels SET current_evaluation_id = $1, updated_at = NOW() WHERE \
     id = $2 RETURNING *",
  )
  .bind(evaluation_id)
  .bind(channel_id)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| CiError::NotFound(format!("Channel {channel_id} not found")))
}

/// Delete a channel.
///
/// # Errors
///
/// Returns error if database delete fails or channel not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let result = sqlx::query("DELETE FROM channels WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await
    .map_err(CiError::Database)?;
  if result.rows_affected() == 0 {
    return Err(CiError::NotFound(format!("Channel {id} not found")));
  }
  Ok(())
}

/// Upsert a channel (insert or update on conflict).
///
/// # Errors
///
/// Returns error if database operation fails.
pub async fn upsert(
  pool: &PgPool,
  project_id: Uuid,
  name: &str,
  jobset_id: Uuid,
) -> Result<Channel> {
  sqlx::query_as::<_, Channel>(
    "INSERT INTO channels (project_id, name, jobset_id) VALUES ($1, $2, $3) \
     ON CONFLICT (project_id, name) DO UPDATE SET jobset_id = \
     EXCLUDED.jobset_id RETURNING *",
  )
  .bind(project_id)
  .bind(name)
  .bind(jobset_id)
  .fetch_one(pool)
  .await
  .map_err(CiError::Database)
}

/// Sync channels from declarative config.
/// Deletes channels not in the declarative list and upserts those that are.
///
/// # Errors
///
/// Returns error if database operations fail.
pub async fn sync_for_project(
  pool: &PgPool,
  project_id: Uuid,
  channels: &[DeclarativeChannel],
  resolve_jobset: impl Fn(&str) -> Option<Uuid>,
) -> Result<()> {
  // Get channel names from declarative config
  let names: Vec<&str> = channels.iter().map(|c| c.name.as_str()).collect();

  // Delete channels not in declarative config
  sqlx::query(
    "DELETE FROM channels WHERE project_id = $1 AND name != ALL($2::text[])",
  )
  .bind(project_id)
  .bind(&names)
  .execute(pool)
  .await
  .map_err(CiError::Database)?;

  // Upsert each channel
  for channel in channels {
    if let Some(jobset_id) = resolve_jobset(&channel.jobset_name) {
      upsert(pool, project_id, &channel.name, jobset_id).await?;
    } else {
      tracing::warn!(
          channel = %channel.name,
          jobset_name = %channel.jobset_name,
          "Could not resolve jobset for declarative channel"
      );
    }
  }

  Ok(())
}

/// Find the channel for a jobset and auto-promote if all builds in the
/// evaluation succeeded.
///
/// # Errors
///
/// Returns error if database operations fail.
pub async fn auto_promote_if_complete(
  pool: &PgPool,
  jobset_id: Uuid,
  evaluation_id: Uuid,
) -> Result<()> {
  // Check if all builds for this evaluation are completed
  let row: (i64, i64) = sqlx::query_as(
    "SELECT COUNT(*), COUNT(*) FILTER (WHERE status = 'succeeded') FROM \
     builds WHERE evaluation_id = $1",
  )
  .bind(evaluation_id)
  .fetch_one(pool)
  .await
  .map_err(CiError::Database)?;

  let (total, completed) = row;
  if total == 0 || total != completed {
    return Ok(());
  }

  // All builds completed, promote to any channels tracking this jobset
  let channels =
    sqlx::query_as::<_, Channel>("SELECT * FROM channels WHERE jobset_id = $1")
      .bind(jobset_id)
      .fetch_all(pool)
      .await
      .map_err(CiError::Database)?;

  for channel in channels {
    match promote(pool, channel.id, evaluation_id).await {
      Ok(_) => {
        tracing::info!(
            channel = %channel.name,
            evaluation_id = %evaluation_id,
            "Auto-promoted evaluation to channel"
        );
      },
      Err(e) => {
        tracing::warn!(
            channel = %channel.name,
            evaluation_id = %evaluation_id,
            "Failed to auto-promote channel: {e}"
        );
      },
    }
  }

  Ok(())
}
