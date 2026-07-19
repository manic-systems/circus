use circus_codegen::queries::evaluations as q;
use uuid::Uuid;

use crate::{
  db::{DbTransaction, PgPool, is_unique_violation},
  error::{CiError, Result},
  models::{
    CreateEvaluation,
    Evaluation,
    EvaluationStatus,
    EvaluationTriggerKind,
  },
};

impl TryFrom<q::EvaluationRow> for Evaluation {
  type Error = CiError;

  fn try_from(r: q::EvaluationRow) -> Result<Self> {
    Ok(Self {
      id:              r.id,
      jobset_id:       r.jobset_id,
      commit_hash:     r.commit_hash,
      evaluation_time: r.evaluation_time,
      status:          r.status.parse().map_err(CiError::Internal)?,
      error_message:   r.error_message,
      inputs_hash:     r.inputs_hash,
      trigger_kind:    r.trigger_kind.parse().map_err(CiError::Internal)?,
      hidden:          r.hidden,
      pr_number:       r.pr_number,
      pr_head_branch:  r.pr_head_branch,
      pr_base_branch:  r.pr_base_branch,
      pr_action:       r.pr_action,
    })
  }
}

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
  let client = pool.get().await?;
  let row = q::create_with_kind()
    .bind(
      &client,
      &input.jobset_id,
      &input.commit_hash,
      &status.as_db_str(),
      &trigger_kind.as_db_str(),
      &input.pr_number,
      &input.pr_head_branch,
      &input.pr_base_branch,
      &input.pr_action,
    )
    .one()
    .await
    .map_err(|e| {
      if is_unique_violation(&e) {
        CiError::Conflict(format!(
          "Evaluation for commit '{}' already exists in this jobset",
          input.commit_hash
        ))
      } else {
        CiError::Database(e)
      }
    })?;
  row.try_into()
}

/// Get an evaluation by ID.
///
/// # Errors
///
/// Returns error if database query fails or evaluation not found.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<Evaluation> {
  let client = pool.get().await?;
  q::get()
    .bind(&client, &id)
    .opt()
    .await?
    .ok_or_else(|| CiError::NotFound(format!("Evaluation {id} not found")))?
    .try_into()
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
  let client = pool.get().await?;
  q::get_visible()
    .bind(&client, &id, &include_hidden)
    .opt()
    .await?
    .ok_or_else(|| CiError::NotFound(format!("Evaluation {id} not found")))?
    .try_into()
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
  let client = pool.get().await?;
  let rows = q::list_for_jobset().bind(&client, &jobset_id).all().await?;
  rows.into_iter().map(Evaluation::try_from).collect()
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
  let client = pool.get().await?;
  let rows = q::list_filtered_with_visibility()
    .bind(
      &client,
      &jobset_id,
      &status,
      &include_hidden,
      &limit,
      &offset,
    )
    .all()
    .await?;
  rows.into_iter().map(Evaluation::try_from).collect()
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
  let client = pool.get().await?;
  Ok(
    q::count_filtered_with_visibility()
      .bind(&client, &jobset_id, &status, &include_hidden)
      .one()
      .await?,
  )
}

/// Filters for the dashboard evaluation list. Name filters match
/// case-insensitive substrings (like the builds job filter); `commit` matches
/// a hash prefix.
#[derive(Debug, Clone, Copy, Default)]
pub struct EvaluationListFilter<'a> {
  pub project:        Option<&'a str>,
  pub jobset:         Option<&'a str>,
  pub commit:         Option<&'a str>,
  pub status:         Option<&'a str>,
  pub include_hidden: bool,
}

/// List evaluations for the dashboard list page, filtered by project/jobset
/// name, commit prefix, and status.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_page_filtered(
  pool: &PgPool,
  filter: EvaluationListFilter<'_>,
  limit: i64,
  offset: i64,
) -> Result<Vec<Evaluation>> {
  Ok(
    sqlx::query_as::<_, Evaluation>(
      "SELECT e.* FROM evaluations e JOIN jobsets j ON j.id = e.jobset_id \
       JOIN projects p ON p.id = j.project_id WHERE ($1::text IS NULL OR \
       p.name ILIKE '%' || $1 || '%') AND ($2::text IS NULL OR j.name ILIKE \
       '%' || $2 || '%') AND ($3::text IS NULL OR e.commit_hash LIKE $3 || \
       '%') AND ($4::text IS NULL OR e.status = $4) AND ($5::boolean OR \
       e.hidden = false) ORDER BY e.evaluation_time DESC LIMIT $6 OFFSET $7",
    )
    .bind(filter.project)
    .bind(filter.jobset)
    .bind(filter.commit)
    .bind(filter.status)
    .bind(filter.include_hidden)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?,
  )
}

/// Count evaluations matching [`EvaluationListFilter`].
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count_page_filtered(
  pool: &PgPool,
  filter: EvaluationListFilter<'_>,
) -> Result<i64> {
  let row: (i64,) = sqlx::query_as(
    "SELECT COUNT(*) FROM evaluations e JOIN jobsets j ON j.id = e.jobset_id \
     JOIN projects p ON p.id = j.project_id WHERE ($1::text IS NULL OR p.name \
     ILIKE '%' || $1 || '%') AND ($2::text IS NULL OR j.name ILIKE '%' || $2 \
     || '%') AND ($3::text IS NULL OR e.commit_hash LIKE $3 || '%') AND \
     ($4::text IS NULL OR e.status = $4) AND ($5::boolean OR e.hidden = false)",
  )
  .bind(filter.project)
  .bind(filter.jobset)
  .bind(filter.commit)
  .bind(filter.status)
  .bind(filter.include_hidden)
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
  let client = pool.get().await?;
  q::set_hidden()
    .bind(&client, &hidden, &id)
    .opt()
    .await?
    .ok_or_else(|| CiError::NotFound(format!("Evaluation {id} not found")))?
    .try_into()
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
  let client = pool.get().await?;
  q::try_claim_pending()
    .bind(&client, &id)
    .opt()
    .await?
    .map(Evaluation::try_from)
    .transpose()
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
  let client = pool.get().await?;
  q::update_status()
    .bind(&client, &status.as_db_str(), &error_message, &id)
    .opt()
    .await?
    .ok_or_else(|| CiError::NotFound(format!("Evaluation {id} not found")))?
    .try_into()
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
  let client = pool.get().await?;
  q::finish_running()
    .bind(&client, &status.as_db_str(), &error_message, &id)
    .opt()
    .await?
    .map(Evaluation::try_from)
    .transpose()
}

/// Cancel an evaluation that has not reached a terminal state.
///
/// # Errors
///
/// Returns an error if the database query fails.
pub async fn cancel(pool: &PgPool, id: Uuid) -> Result<Option<Evaluation>> {
  let client = pool.get().await?;
  q::cancel()
    .bind(&client, &id)
    .opt()
    .await?
    .map(Evaluation::try_from)
    .transpose()
}

/// Requeue a cancelled, failed, or timed-out evaluation after discarding its
/// stale builds. A disabled jobset rejects the restart; a retained one-shot
/// jobset is re-enabled for its new attempt.
///
/// # Errors
///
/// Returns an error if the database transaction fails.
pub async fn restart(pool: &PgPool, id: Uuid) -> Result<Option<Evaluation>> {
  let mut client = pool.get().await?;
  let tx = client.transaction().await?;
  let evaluation = q::restart_requeue()
    .bind(&tx, &id)
    .opt()
    .await?
    .map(Evaluation::try_from)
    .transpose()?;

  if evaluation.is_some() {
    q::restart_delete_builds().bind(&tx, &id).await?;
    q::restart_reenable_one_shot().bind(&tx, &id).await?;
  }
  tx.commit().await?;
  Ok(evaluation)
}

/// Lock a running evaluation before atomically persisting its result.
///
/// # Errors
///
/// Returns an error if the database query fails.
pub async fn lock_running(tx: &DbTransaction<'_>, id: Uuid) -> Result<bool> {
  Ok(q::lock_running().bind(tx, &id).opt().await?.is_some())
}

/// Finish a locked running evaluation within its result transaction.
///
/// # Errors
///
/// Returns an error if the database query fails.
pub async fn finish_running_in_transaction(
  tx: &DbTransaction<'_>,
  id: Uuid,
  status: EvaluationStatus,
  error_message: Option<&str>,
) -> Result<bool> {
  Ok(
    q::finish_running()
      .bind(tx, &status.as_db_str(), &error_message, &id)
      .opt()
      .await?
      .is_some(),
  )
}

/// Return whether an evaluator should cancel its currently-running work.
///
/// # Errors
///
/// Returns an error if the database query fails.
pub async fn is_cancelled(pool: &PgPool, id: Uuid) -> Result<bool> {
  let client = pool.get().await?;
  let status: Option<String> = q::status_of().bind(&client, &id).opt().await?;
  Ok(
    status
      .is_some_and(|status| status == EvaluationStatus::Cancelled.as_db_str()),
  )
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
  let client = pool.get().await?;
  q::get_latest()
    .bind(&client, &jobset_id)
    .opt()
    .await?
    .map(Evaluation::try_from)
    .transpose()
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
  let client = pool.get().await?;
  q::set_inputs_hash().bind(&client, &hash, &id).await?;
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
  let client = pool.get().await?;
  q::get_by_inputs_hash()
    .bind(&client, &jobset_id, &inputs_hash)
    .opt()
    .await?
    .map(Evaluation::try_from)
    .transpose()
}

/// Count total evaluations.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count(pool: &PgPool) -> Result<i64> {
  let client = pool.get().await?;
  Ok(q::count().bind(&client).one().await?)
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
  let client = pool.get().await?;
  let rows = q::list_pending().bind(&client).all().await?;
  rows.into_iter().map(Evaluation::try_from).collect()
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
  let client = pool.get().await?;
  Ok(q::list_jobsets_with_pending().bind(&client).all().await?)
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
  let client = pool.get().await?;
  q::get_by_jobset_and_commit()
    .bind(&client, &jobset_id, &commit_hash)
    .opt()
    .await?
    .map(Evaluation::try_from)
    .transpose()
}

/// Project and jobset identity for an evaluation, used to annotate build
/// listings without a per-build join.
#[derive(Debug, Clone)]
pub struct BuildContext {
  pub evaluation_id: Uuid,
  pub project_id:    Uuid,
  pub project_name:  String,
  pub jobset_id:     Uuid,
  pub jobset_name:   String,
}

/// Resolve project and jobset context for a set of evaluation IDs.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get_build_contexts(
  pool: &PgPool,
  evaluation_ids: &[Uuid],
) -> Result<Vec<BuildContext>> {
  if evaluation_ids.is_empty() {
    return Ok(Vec::new());
  }
  let client = pool.get().await?;
  let rows = q::get_build_contexts()
    .bind(&client, &evaluation_ids)
    .all()
    .await?;
  Ok(
    rows
      .into_iter()
      .map(|r| {
        BuildContext {
          evaluation_id: r.evaluation_id,
          project_id:    r.project_id,
          project_name:  r.project_name,
          jobset_id:     r.jobset_id,
          jobset_name:   r.jobset_name,
        }
      })
      .collect(),
  )
}
