use circus_common::{
  PgPool,
  models::{Evaluation, EvaluationStatus},
  repo,
};

pub enum ExistingEvaluationClaim {
  Claimed(Evaluation),
  Completed { build_count: i64 },
  Running,
  Cancelled,
}

/// Claim an existing evaluation for a fresh attempt when its state permits it.
pub async fn claim_existing(
  pool: &PgPool,
  existing: Evaluation,
) -> color_eyre::Result<ExistingEvaluationClaim> {
  match existing.status {
    EvaluationStatus::Pending => {
      Ok(
        repo::evaluations::try_claim_pending(pool, existing.id)
          .await?
          .map_or(
            ExistingEvaluationClaim::Running,
            ExistingEvaluationClaim::Claimed,
          ),
      )
    },
    EvaluationStatus::Failed | EvaluationStatus::TimedOut => {
      if repo::evaluations::restart(pool, existing.id)
        .await?
        .is_none()
      {
        return Ok(ExistingEvaluationClaim::Running);
      }
      Ok(
        repo::evaluations::try_claim_pending(pool, existing.id)
          .await?
          .map_or(
            ExistingEvaluationClaim::Running,
            ExistingEvaluationClaim::Claimed,
          ),
      )
    },
    EvaluationStatus::Completed => {
      let build_count =
        repo::builds::count_filtered(pool, Some(existing.id), None, None, None)
          .await?;
      if build_count > 0 {
        return Ok(ExistingEvaluationClaim::Completed { build_count });
      }
      Ok(ExistingEvaluationClaim::Claimed(
        repo::evaluations::update_status(
          pool,
          existing.id,
          EvaluationStatus::Running,
          None,
        )
        .await?,
      ))
    },
    EvaluationStatus::Running => Ok(ExistingEvaluationClaim::Running),
    EvaluationStatus::Cancelled => Ok(ExistingEvaluationClaim::Cancelled),
  }
}
