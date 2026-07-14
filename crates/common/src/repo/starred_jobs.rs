//! Starred jobs repository - for personalized dashboard

use circus_codegen::queries::starred_jobs as q;
use uuid::Uuid;

use crate::{
  db::{PgPool, is_unique_violation},
  error::{CiError, Result},
  models::{CreateStarredJob, StarredJob},
};

impl From<q::StarredJobRow> for StarredJob {
  fn from(r: q::StarredJobRow) -> Self {
    Self {
      id:         r.id,
      user_id:    r.user_id,
      project_id: r.project_id,
      jobset_id:  r.jobset_id,
      job_name:   r.job_name,
      created_at: r.created_at,
    }
  }
}

/// Create a new starred job
///
/// # Errors
///
/// Returns error if database insert fails or job already starred.
pub async fn create(
  pool: &PgPool,
  user_id: Uuid,
  data: &CreateStarredJob,
) -> Result<StarredJob> {
  let client = pool.get().await?;
  q::create()
    .bind(
      &client,
      &user_id,
      &data.project_id,
      &data.jobset_id,
      &data.job_name,
    )
    .one()
    .await
    .map(StarredJob::from)
    .map_err(|e| {
      if is_unique_violation(&e) {
        CiError::Conflict("Job already starred".to_string())
      } else {
        CiError::Database(e)
      }
    })
}

/// Get a starred job by ID
///
/// # Errors
///
/// Returns error if database query fails or starred job not found.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<StarredJob> {
  let client = pool.get().await?;
  q::get()
    .bind(&client, &id)
    .opt()
    .await?
    .map(StarredJob::from)
    .ok_or_else(|| CiError::NotFound(format!("Starred job {id} not found")))
}

/// List starred jobs for a user with pagination
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_for_user(
  pool: &PgPool,
  user_id: Uuid,
  limit: i64,
  offset: i64,
) -> Result<Vec<StarredJob>> {
  let client = pool.get().await?;
  let rows = q::list_for_user()
    .bind(&client, &user_id, &limit, &offset)
    .all()
    .await?;
  Ok(rows.into_iter().map(StarredJob::from).collect())
}

/// Count starred jobs for a user
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count_for_user(pool: &PgPool, user_id: Uuid) -> Result<i64> {
  let client = pool.get().await?;
  Ok(q::count_for_user().bind(&client, &user_id).one().await?)
}

/// Check if a user has starred a specific job
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn is_starred(
  pool: &PgPool,
  user_id: Uuid,
  project_id: Uuid,
  jobset_id: Option<Uuid>,
  job_name: &str,
) -> Result<bool> {
  let client = pool.get().await?;
  let count = q::is_starred()
    .bind(&client, &user_id, &project_id, &jobset_id, &job_name)
    .one()
    .await?;
  Ok(count > 0)
}

/// Delete a starred job
///
/// # Errors
///
/// Returns error if database delete fails or starred job not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let client = pool.get().await?;
  let affected = q::delete().bind(&client, &id).await?;
  if affected == 0 {
    return Err(CiError::NotFound(format!("Starred job {id} not found")));
  }
  Ok(())
}

/// Delete a starred job by ID for a specific user.
///
/// # Errors
///
/// Returns error if database delete fails or the user's starred job does not
/// exist.
pub async fn delete_for_user(
  pool: &PgPool,
  user_id: Uuid,
  id: Uuid,
) -> Result<()> {
  let client = pool.get().await?;
  let affected = q::delete_for_user().bind(&client, &id, &user_id).await?;
  if affected == 0 {
    return Err(CiError::NotFound(format!("Starred job {id} not found")));
  }
  Ok(())
}

/// Delete a starred job by user and job details
///
/// # Errors
///
/// Returns error if database delete fails or starred job not found.
pub async fn delete_by_job(
  pool: &PgPool,
  user_id: Uuid,
  project_id: Uuid,
  jobset_id: Option<Uuid>,
  job_name: &str,
) -> Result<()> {
  let client = pool.get().await?;
  let affected = q::delete_by_job()
    .bind(&client, &user_id, &project_id, &jobset_id, &job_name)
    .await?;
  if affected == 0 {
    return Err(CiError::NotFound("Starred job not found".to_string()));
  }
  Ok(())
}

/// Delete all starred jobs for a user (when user is deleted)
///
/// # Errors
///
/// Returns error if database delete fails.
pub async fn delete_all_for_user(pool: &PgPool, user_id: Uuid) -> Result<()> {
  let client = pool.get().await?;
  q::delete_all_for_user().bind(&client, &user_id).await?;
  Ok(())
}
