use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  error::{CiError, Result, SqlxResultExt},
  models::{
    CreateEvaluation,
    Evaluation,
    EvaluationStatus,
    EvaluationTriggerKind,
  },
};

/// Create a new evaluation in pending state.
///
/// # Errors
///
/// Returns error if database insert fails or evaluation already exists.
pub async fn create(
  pool: &PgPool,
  input: CreateEvaluation,
) -> Result<Evaluation> {
  create_with_kind(
    pool,
    input,
    EvaluationTriggerKind::SourceChange,
    EvaluationStatus::Pending,
  )
  .await
}

/// Create a manually-triggered evaluation in pending state.
///
/// # Errors
///
/// Returns error if database insert fails or evaluation already exists.
pub async fn create_manual(
  pool: &PgPool,
  input: CreateEvaluation,
) -> Result<Evaluation> {
  create_with_kind(
    pool,
    input,
    EvaluationTriggerKind::Manual,
    EvaluationStatus::Pending,
  )
  .await
}

/// Create a source-change poll evaluation that is already being processed.
///
/// # Errors
///
/// Returns error if database insert fails or evaluation already exists.
pub async fn create_running_source_change(
  pool: &PgPool,
  input: CreateEvaluation,
) -> Result<Evaluation> {
  create_with_kind(
    pool,
    input,
    EvaluationTriggerKind::SourceChange,
    EvaluationStatus::Running,
  )
  .await
}

/// Create an interval-triggered evaluation that is already being processed.
///
/// Interval evaluations intentionally do not deduplicate on commit hash: each
/// interval tick is a fresh CI run for the same jobset state.
///
/// # Errors
///
/// Returns error if database insert fails.
pub async fn create_interval(
  pool: &PgPool,
  input: CreateEvaluation,
) -> Result<Evaluation> {
  create_with_kind(
    pool,
    input,
    EvaluationTriggerKind::Interval,
    EvaluationStatus::Running,
  )
  .await
}

async fn create_with_kind(
  pool: &PgPool,
  input: CreateEvaluation,
  trigger_kind: EvaluationTriggerKind,
  status: EvaluationStatus,
) -> Result<Evaluation> {
  sqlx::query_as::<_, Evaluation>(
    "INSERT INTO evaluations (jobset_id, commit_hash, status, trigger_kind, \
     pr_number, pr_head_branch, pr_base_branch, pr_action) VALUES ($1, $2, \
     $3, $4, $5, $6, $7, $8) RETURNING *",
  )
  .bind(input.jobset_id)
  .bind(&input.commit_hash)
  .bind(status)
  .bind(trigger_kind)
  .bind(input.pr_number)
  .bind(&input.pr_head_branch)
  .bind(&input.pr_base_branch)
  .bind(&input.pr_action)
  .fetch_one(pool)
  .await
  .on_unique_violation(|| {
    format!(
      "Evaluation for commit '{}' already exists in this jobset",
      input.commit_hash
    )
  })
}

/// Get an evaluation by ID.
///
/// # Errors
///
/// Returns error if database query fails or evaluation not found.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<Evaluation> {
  sqlx::query_as::<_, Evaluation>("SELECT * FROM evaluations WHERE id = $1")
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| CiError::NotFound(format!("Evaluation {id} not found")))
}

/// Get an evaluation by ID, optionally allowing hidden rows.
///
/// # Errors
///
/// Returns error if database query fails or evaluation not found.
pub async fn get_visible(
  pool: &PgPool,
  id: Uuid,
  include_hidden: bool,
) -> Result<Evaluation> {
  sqlx::query_as::<_, Evaluation>(
    "SELECT * FROM evaluations WHERE id = $1 AND ($2::boolean OR hidden = \
     false)",
  )
  .bind(id)
  .bind(include_hidden)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| CiError::NotFound(format!("Evaluation {id} not found")))
}

/// List all evaluations for a jobset.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_for_jobset(
  pool: &PgPool,
  jobset_id: Uuid,
) -> Result<Vec<Evaluation>> {
  Ok(
    sqlx::query_as::<_, Evaluation>(
      "SELECT * FROM evaluations WHERE jobset_id = $1 ORDER BY \
       evaluation_time DESC",
    )
    .bind(jobset_id)
    .fetch_all(pool)
    .await?,
  )
}

/// List evaluations with optional `jobset_id` and status filters, with
/// pagination.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_filtered(
  pool: &PgPool,
  jobset_id: Option<Uuid>,
  status: Option<&str>,
  limit: i64,
  offset: i64,
) -> Result<Vec<Evaluation>> {
  list_filtered_with_visibility(pool, jobset_id, status, limit, offset, false)
    .await
}

/// List evaluations with optional filters, optionally including hidden rows.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_filtered_with_visibility(
  pool: &PgPool,
  jobset_id: Option<Uuid>,
  status: Option<&str>,
  limit: i64,
  offset: i64,
  include_hidden: bool,
) -> Result<Vec<Evaluation>> {
  Ok(
    sqlx::query_as::<_, Evaluation>(
      "SELECT * FROM evaluations WHERE ($1::uuid IS NULL OR jobset_id = $1) \
       AND ($2::text IS NULL OR status = $2) AND ($5::boolean OR hidden = \
       false) ORDER BY evaluation_time DESC LIMIT $3 OFFSET $4",
    )
    .bind(jobset_id)
    .bind(status)
    .bind(limit)
    .bind(offset)
    .bind(include_hidden)
    .fetch_all(pool)
    .await?,
  )
}

/// Count evaluations matching filter criteria.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count_filtered(
  pool: &PgPool,
  jobset_id: Option<Uuid>,
  status: Option<&str>,
) -> Result<i64> {
  count_filtered_with_visibility(pool, jobset_id, status, false).await
}

/// Count evaluations matching filter criteria, optionally including hidden
/// rows.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count_filtered_with_visibility(
  pool: &PgPool,
  jobset_id: Option<Uuid>,
  status: Option<&str>,
  include_hidden: bool,
) -> Result<i64> {
  let row: (i64,) = sqlx::query_as(
    "SELECT COUNT(*) FROM evaluations WHERE ($1::uuid IS NULL OR jobset_id = \
     $1) AND ($2::text IS NULL OR status = $2) AND ($3::boolean OR hidden = \
     false)",
  )
  .bind(jobset_id)
  .bind(status)
  .bind(include_hidden)
  .fetch_one(pool)
  .await?;
  Ok(row.0)
}

/// Hide or unhide an evaluation in dashboard listings.
///
/// Hidden evaluations remain in the database and continue to count for build
/// history, but non-admin dashboard/API views omit them.
///
/// # Errors
///
/// Returns error if database update fails or evaluation not found.
pub async fn set_hidden(
  pool: &PgPool,
  id: Uuid,
  hidden: bool,
) -> Result<Evaluation> {
  sqlx::query_as::<_, Evaluation>(
    "UPDATE evaluations SET hidden = $1 WHERE id = $2 RETURNING *",
  )
  .bind(hidden)
  .bind(id)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| CiError::NotFound(format!("Evaluation {id} not found")))
}

/// Atomically transition an evaluation from `pending` to `running`.
/// Returns the updated row if the transition succeeded, or `None` if the
/// evaluation was no longer pending (already claimed, completed, or failed).
///
/// Used by the evaluator to claim push-driven work and avoid double-processing
/// when multiple NOTIFY wake-ups land for the same row.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn try_claim_pending(
  pool: &PgPool,
  id: Uuid,
) -> Result<Option<Evaluation>> {
  Ok(
    sqlx::query_as::<_, Evaluation>(
      "UPDATE evaluations SET status = 'running' WHERE id = $1 AND status = \
       'pending' RETURNING *",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?,
  )
}

/// Update evaluation status and optional error message.
///
/// # Errors
///
/// Returns error if database update fails or evaluation not found.
pub async fn update_status(
  pool: &PgPool,
  id: Uuid,
  status: EvaluationStatus,
  error_message: Option<&str>,
) -> Result<Evaluation> {
  sqlx::query_as::<_, Evaluation>(
    "UPDATE evaluations SET status = $1, error_message = $2 WHERE id = $3 \
     RETURNING *",
  )
  .bind(status)
  .bind(error_message)
  .bind(id)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| CiError::NotFound(format!("Evaluation {id} not found")))
}

/// Finish an evaluation only while this evaluator still owns its running state.
///
/// # Errors
///
/// Returns an error if the database query fails.
pub async fn finish_running(
  pool: &PgPool,
  id: Uuid,
  status: EvaluationStatus,
  error_message: Option<&str>,
) -> Result<Option<Evaluation>> {
  Ok(
    sqlx::query_as::<_, Evaluation>(
      "UPDATE evaluations SET status = $1, error_message = $2 WHERE id = $3 \
       AND status = 'running' RETURNING *",
    )
    .bind(status)
    .bind(error_message)
    .bind(id)
    .fetch_optional(pool)
    .await?,
  )
}

/// Cancel an evaluation that has not reached a terminal state.
///
/// # Errors
///
/// Returns an error if the database query fails.
pub async fn cancel(pool: &PgPool, id: Uuid) -> Result<Option<Evaluation>> {
  Ok(
    sqlx::query_as::<_, Evaluation>(
      "UPDATE evaluations SET status = 'cancelled', error_message = NULL \
       WHERE id = $1 AND status IN ('pending', 'running') RETURNING *",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?,
  )
}

/// Requeue a cancelled or failed evaluation after discarding its stale builds.
///
/// # Errors
///
/// Returns an error if the database transaction fails.
pub async fn restart(pool: &PgPool, id: Uuid) -> Result<Option<Evaluation>> {
  let mut tx = pool.begin().await?;
  let evaluation = sqlx::query_as::<_, Evaluation>(
    "UPDATE evaluations SET status = 'pending', error_message = NULL, \
     inputs_hash = NULL WHERE id = $1 AND status IN ('cancelled', 'failed') \
     RETURNING *",
  )
  .bind(id)
  .fetch_optional(&mut *tx)
  .await?;

  if evaluation.is_some() {
    sqlx::query("DELETE FROM builds WHERE evaluation_id = $1")
      .bind(id)
      .execute(&mut *tx)
      .await?;
  }
  tx.commit().await?;
  Ok(evaluation)
}

/// Return whether an evaluator should cancel its currently-running work.
///
/// # Errors
///
/// Returns an error if the database query fails.
pub async fn is_cancelled(pool: &PgPool, id: Uuid) -> Result<bool> {
  let status = sqlx::query_scalar::<_, EvaluationStatus>(
    "SELECT status FROM evaluations WHERE id = $1",
  )
  .bind(id)
  .fetch_optional(pool)
  .await?;
  Ok(status.is_some_and(|status| status == EvaluationStatus::Cancelled))
}

/// Get the latest completed evaluation for a jobset.
///
/// Only completed evaluations are returned. Failed or running evaluations are
/// excluded so that a previously-failed evaluation does not permanently block
/// re-evaluation of the same commit via the inputs-hash cache check.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get_latest(
  pool: &PgPool,
  jobset_id: Uuid,
) -> Result<Option<Evaluation>> {
  Ok(
    sqlx::query_as::<_, Evaluation>(
      "SELECT * FROM evaluations WHERE jobset_id = $1 AND status = \
       'completed' ORDER BY evaluation_time DESC LIMIT 1",
    )
    .bind(jobset_id)
    .fetch_optional(pool)
    .await?,
  )
}

/// Set the inputs hash for an evaluation (used for eval caching).
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn set_inputs_hash(
  pool: &PgPool,
  id: Uuid,
  hash: &str,
) -> Result<()> {
  sqlx::query("UPDATE evaluations SET inputs_hash = $1 WHERE id = $2")
    .bind(hash)
    .bind(id)
    .execute(pool)
    .await?;
  Ok(())
}

/// Check if an evaluation with the same `inputs_hash` already exists for this
/// jobset.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get_by_inputs_hash(
  pool: &PgPool,
  jobset_id: Uuid,
  inputs_hash: &str,
) -> Result<Option<Evaluation>> {
  Ok(
    sqlx::query_as::<_, Evaluation>(
      "SELECT * FROM evaluations WHERE jobset_id = $1 AND inputs_hash = $2 \
       AND status = 'completed' ORDER BY evaluation_time DESC LIMIT 1",
    )
    .bind(jobset_id)
    .bind(inputs_hash)
    .fetch_optional(pool)
    .await?,
  )
}

/// Count total evaluations.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count(pool: &PgPool) -> Result<i64> {
  let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM evaluations")
    .fetch_one(pool)
    .await?;
  Ok(row.0)
}

/// List all pending evaluations, oldest first. The evaluator drains
/// this queue every cycle: each row is push-driven work (webhook commit
/// or `/evaluations/trigger` call) that must run at its declared
/// `commit_hash`, independent of jobset polling.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_pending(pool: &PgPool) -> Result<Vec<Evaluation>> {
  Ok(
    sqlx::query_as::<_, Evaluation>(
      "SELECT * FROM evaluations WHERE status = 'pending' ORDER BY \
       evaluation_time ASC",
    )
    .fetch_all(pool)
    .await?,
  )
}

/// List jobset IDs with at least one pending evaluation.
///
/// Used by the evaluator to find jobsets that have explicit push-driven
/// work waiting (webhook commits, manual /evaluations/trigger calls).
/// These bypass the periodic `check_interval` poll because the work was
/// pushed in, not discovered by git polling.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_jobsets_with_pending(pool: &PgPool) -> Result<Vec<Uuid>> {
  let rows: Vec<(Uuid,)> = sqlx::query_as(
    "SELECT DISTINCT jobset_id FROM evaluations WHERE status = 'pending'",
  )
  .fetch_all(pool)
  .await?;
  Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Get an evaluation by `jobset_id` and `commit_hash`.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get_by_jobset_and_commit(
  pool: &PgPool,
  jobset_id: Uuid,
  commit_hash: &str,
) -> Result<Option<Evaluation>> {
  Ok(
    sqlx::query_as::<_, Evaluation>(
      "SELECT * FROM evaluations WHERE jobset_id = $1 AND commit_hash = $2 \
       ORDER BY (trigger_kind = 'interval') ASC, evaluation_time DESC LIMIT 1",
    )
    .bind(jobset_id)
    .bind(commit_hash)
    .fetch_optional(pool)
    .await?,
  )
}
