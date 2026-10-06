//! `herculesCI.onSchedule` jobs discovered on a jobset's default branch.

use circus_codegen::queries::jobset_schedules as q;
use jiff::Timestamp;
use uuid::Uuid;

use crate::{db::PgPool, error::Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobsetSchedule {
  pub jobset_id:     Uuid,
  pub name:          String,
  pub when_spec:     serde_json::Value,
  pub commit_hash:   String,
  pub next_due_at:   Timestamp,
  pub last_fired_at: Option<Timestamp>,
}

impl From<q::JobsetScheduleRow> for JobsetSchedule {
  fn from(row: q::JobsetScheduleRow) -> Self {
    Self {
      jobset_id:     row.jobset_id,
      name:          row.name,
      when_spec:     row.when_spec,
      commit_hash:   row.commit_hash,
      next_due_at:   row.next_due_at,
      last_fired_at: row.last_fired_at,
    }
  }
}

/// List a jobset's schedules.
///
/// # Errors
///
/// Returns an error if the query fails.
pub async fn list_for_jobset(
  pool: &PgPool,
  jobset_id: Uuid,
) -> Result<Vec<JobsetSchedule>> {
  let client = pool.get().await?;
  Ok(
    q::list_for_jobset()
      .bind(&client, &jobset_id)
      .all()
      .await?
      .into_iter()
      .map(JobsetSchedule::from)
      .collect(),
  )
}

/// Insert or refresh one schedule.
///
/// # Errors
///
/// Returns an error if the write fails.
pub async fn upsert(
  pool: &PgPool,
  jobset_id: Uuid,
  name: &str,
  when_spec: &serde_json::Value,
  commit_hash: &str,
  next_due_at: Timestamp,
) -> Result<()> {
  let client = pool.get().await?;
  q::upsert()
    .bind(
      &client,
      &jobset_id,
      &name,
      when_spec,
      &commit_hash,
      &next_due_at,
    )
    .await?;
  Ok(())
}

/// Drop every schedule of the jobset not named in `keep`.
///
/// # Errors
///
/// Returns an error if the write fails.
pub async fn delete_except(
  pool: &PgPool,
  jobset_id: Uuid,
  keep: &[&str],
) -> Result<()> {
  let client = pool.get().await?;
  q::delete_except().bind(&client, &jobset_id, &keep).await?;
  Ok(())
}

/// Schedules whose next run is due.
///
/// # Errors
///
/// Returns an error if the query fails.
pub async fn list_due(pool: &PgPool) -> Result<Vec<JobsetSchedule>> {
  let client = pool.get().await?;
  Ok(
    q::list_due()
      .bind(&client)
      .all()
      .await?
      .into_iter()
      .map(JobsetSchedule::from)
      .collect(),
  )
}

/// Record a run and move the schedule to `next_due_at`.
///
/// # Errors
///
/// Returns an error if the write fails.
pub async fn mark_fired(
  pool: &PgPool,
  schedule: &JobsetSchedule,
  next_due_at: Timestamp,
) -> Result<bool> {
  let client = pool.get().await?;
  Ok(
    q::mark_fired()
      .bind(
        &client,
        &next_due_at,
        &schedule.jobset_id,
        &schedule.name,
        &schedule.next_due_at,
      )
      .opt()
      .await?
      .is_some(),
  )
}
