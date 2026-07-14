use circus_codegen::queries::channels as q;
use circus_config::DeclarativeChannel;
use uuid::Uuid;

use crate::{
  db::{PgPool, is_unique_violation},
  error::{CiError, Result},
  models::{Channel, CreateChannel},
};

impl From<q::ChannelRow> for Channel {
  fn from(r: q::ChannelRow) -> Self {
    Self {
      id:                    r.id,
      project_id:            r.project_id,
      name:                  r.name,
      jobset_id:             r.jobset_id,
      current_evaluation_id: r.current_evaluation_id,
      created_at:            r.created_at,
      updated_at:            r.updated_at,
    }
  }
}

/// Create a release channel.
///
/// # Errors
///
/// Returns error if database insert fails or channel already exists.
pub async fn create(pool: &PgPool, input: CreateChannel) -> Result<Channel> {
  let client = pool.get().await?;
  q::create()
    .bind(&client, &input.project_id, &input.name, &input.jobset_id)
    .one()
    .await
    .map(Channel::from)
    .map_err(|e| {
      if is_unique_violation(&e) {
        CiError::Conflict(format!(
          "Channel '{}' already exists for this project",
          input.name
        ))
      } else {
        CiError::Database(e)
      }
    })
}

/// Get a channel by ID.
///
/// # Errors
///
/// Returns error if database query fails or channel not found.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<Channel> {
  let client = pool.get().await?;
  q::get()
    .bind(&client, &id)
    .opt()
    .await?
    .map(Channel::from)
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
  let client = pool.get().await?;
  let rows = q::list_for_project()
    .bind(&client, &project_id)
    .all()
    .await?;
  Ok(rows.into_iter().map(Channel::from).collect())
}

/// List all channels.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_all(pool: &PgPool) -> Result<Vec<Channel>> {
  let client = pool.get().await?;
  let rows = q::list_all().bind(&client).all().await?;
  Ok(rows.into_iter().map(Channel::from).collect())
}

/// Count all channels.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count(pool: &PgPool) -> Result<i64> {
  let client = pool.get().await?;
  Ok(q::count().bind(&client).one().await?)
}

/// Look up a channel by name. Names are unique within a project, but channel
/// manifest URLs are typically resolved by name only; the newest match wins
/// when multiple projects share the same channel name.
///
/// # Errors
///
/// Returns error if the database query fails or no channel matches.
pub async fn get_by_name(pool: &PgPool, name: &str) -> Result<Channel> {
  let client = pool.get().await?;
  q::get_by_name()
    .bind(&client, &name)
    .opt()
    .await?
    .map(Channel::from)
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
  let client = pool.get().await?;
  q::promote()
    .bind(&client, &evaluation_id, &channel_id)
    .opt()
    .await?
    .map(Channel::from)
    .ok_or_else(|| CiError::NotFound(format!("Channel {channel_id} not found")))
}

/// Delete a channel.
///
/// # Errors
///
/// Returns error if database delete fails or channel not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let client = pool.get().await?;
  let affected = q::delete().bind(&client, &id).await?;
  if affected == 0 {
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
  let client = pool.get().await?;
  Ok(
    q::upsert()
      .bind(&client, &project_id, &name, &jobset_id)
      .one()
      .await
      .map(Channel::from)?,
  )
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
  {
    let client = pool.get().await?;
    q::sync_for_project_delete()
      .bind(&client, &project_id, &names)
      .await?;
  }

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
  let channels = {
    let client = pool.get().await?;
    // Check if all builds for this evaluation are completed
    let counts = q::auto_promote_count()
      .bind(&client, &evaluation_id)
      .one()
      .await?;

    let total = counts.total.unwrap_or(0);
    let completed = counts.completed.unwrap_or(0);
    if total == 0 || total != completed {
      return Ok(());
    }

    // All builds completed, promote to any channels tracking this jobset
    q::auto_promote_channels()
      .bind(&client, &jobset_id)
      .all()
      .await?
      .into_iter()
      .map(Channel::from)
      .collect::<Vec<_>>()
  };

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
