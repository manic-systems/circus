use circus_common::{
  PgPool,
  models::{Build, EvaluationTriggerKind, Project},
  repo,
};

pub async fn get_project_for_build(
  pool: &PgPool,
  build: &Build,
) -> Option<(Project, String)> {
  let eval = repo::evaluations::get(pool, build.evaluation_id)
    .await
    .ok()?;
  let jobset = repo::jobsets::get(pool, eval.jobset_id).await.ok()?;
  let project = repo::projects::get(pool, jobset.project_id).await.ok()?;
  Some((project, eval.commit_hash))
}

pub async fn is_interval_rebuild(pool: &PgPool, build: &Build) -> bool {
  match repo::evaluations::get(pool, build.evaluation_id).await {
    Ok(eval) => eval.trigger_kind == EvaluationTriggerKind::Interval,
    Err(e) => {
      tracing::warn!(
        build_id = %build.id,
        evaluation_id = %build.evaluation_id,
        "Failed to load evaluation trigger kind: {e}"
      );
      false
    },
  }
}
