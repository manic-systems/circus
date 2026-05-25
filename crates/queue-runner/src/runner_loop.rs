use std::{sync::Arc, time::Duration};

use circus_common::{
  config::HotConfig,
  models::{Build, BuildStatus, JobsetState},
  repo,
};
use sqlx::PgPool;
use tokio::sync::{Notify, RwLock};

use crate::worker::WorkerPool;

/// Fetch project and commit hash for a build by traversing:
///
/// Build -> Evaluation -> Jobset -> Project.
async fn get_project_for_build(
  pool: &PgPool,
  build: &Build,
) -> Option<(circus_common::models::Project, String)> {
  let eval = repo::evaluations::get(pool, build.evaluation_id)
    .await
    .ok()?;
  let jobset = repo::jobsets::get(pool, eval.jobset_id).await.ok()?;
  let project = repo::projects::get(pool, jobset.project_id).await.ok()?;
  Some((project, eval.commit_hash))
}

/// Main queue runner loop. Polls for pending builds and dispatches them to
/// workers.
///
/// # Errors
///
/// Returns error if database operations fail and `strict_errors` is enabled.
pub async fn run(
  pool: PgPool,
  worker_pool: Arc<WorkerPool>,
  hot_config: Arc<RwLock<HotConfig>>,
  wakeup: Arc<Notify>,
  strict_errors: bool,
  failed_paths_cache: bool,
  unsupported_timeout: Option<Duration>,
) -> anyhow::Result<()> {
  // Reset orphaned builds from previous crashes (older than 5 minutes)
  match repo::builds::reset_orphaned(&pool, 300).await {
    Ok(count) if count > 0 => {
      tracing::warn!(count, "Reset orphaned builds back to pending");
    },
    Ok(_) => {},
    Err(e) => {
      tracing::error!("Failed to reset orphaned builds: {e}");
    },
  }

  loop {
    let (
      poll_interval,
      notifications_config,
      scheduling_strategy,
      _psi_threshold,
      _psi_check_timeout,
    ) = {
      let hot = hot_config.read().await;
      (
        hot.poll_interval,
        hot.notifications_config.clone(),
        hot.scheduling_strategy.clone(),
        hot.psi_threshold,
        hot.psi_check_timeout,
      )
    };

    let wc = worker_pool.worker_count() as i32;
    match repo::builds::list_pending(&pool, 10, wc).await {
      Ok(builds) => {
        if !builds.is_empty() {
          tracing::info!("Found {} pending builds", builds.len());
        }
        for build in builds {
          // Aggregate builds: check if all constituents are done
          if build.is_aggregate {
            match repo::build_dependencies::all_deps_completed(&pool, build.id)
              .await
            {
              Ok(true) => {
                // All constituents done, mark aggregate as completed
                tracing::info!(
                    build_id = %build.id,
                    job = %build.job_name,
                    "Aggregate build: all constituents completed"
                );
                if let Err(e) = repo::builds::start(&pool, build.id).await {
                  tracing::warn!(build_id = %build.id, "Failed to start aggregate build: {e}");
                }
                if let Err(e) = repo::builds::complete(
                  &pool,
                  build.id,
                  BuildStatus::Succeeded,
                  None,
                  None,
                  None,
                )
                .await
                {
                  tracing::warn!(build_id = %build.id, "Failed to complete aggregate build: {e}");
                  continue;
                }

                // Dispatch completion notification for aggregate build
                if let Ok(updated_build) =
                  repo::builds::get(&pool, build.id).await
                  && let Some((project, commit_hash)) =
                    get_project_for_build(&pool, &updated_build).await
                {
                  circus_common::notifications::dispatch_build_finished(
                    Some(&pool),
                    &updated_build,
                    &project,
                    &commit_hash,
                    &notifications_config,
                  )
                  .await;
                }
                continue;
              },
              Ok(false) => {
                tracing::debug!(
                    build_id = %build.id,
                    "Aggregate build waiting for constituents"
                );
                continue;
              },
              Err(e) => {
                tracing::error!(
                    build_id = %build.id,
                    "Failed to check aggregate deps: {e}"
                );
                continue;
              },
            }
          }

          // Derivation deduplication: reuse result if same drv was already
          // built
          match repo::builds::get_completed_by_drv_path(&pool, &build.drv_path)
            .await
          {
            Ok(Some(existing)) if existing.id != build.id => {
              tracing::info!(
                  build_id = %build.id,
                  existing_id = %existing.id,
                  drv = %build.drv_path,
                  "Dedup: reusing result from existing build"
              );
              if let Err(e) = repo::builds::start(&pool, build.id).await {
                tracing::warn!(build_id = %build.id, "Failed to start dedup build: {e}");
              }
              if let Err(e) = repo::builds::complete(
                &pool,
                build.id,
                BuildStatus::Succeeded,
                existing.log_path.as_deref(),
                existing.build_output_path.as_deref(),
                None,
              )
              .await
              {
                tracing::warn!(build_id = %build.id, "Failed to complete dedup build: {e}");
              }
              continue;
            },
            _ => {},
          }

          // Failed paths cache: skip known-failing derivations
          if failed_paths_cache
            && matches!(
              repo::failed_paths_cache::is_cached_failure(
                &pool,
                &build.drv_path,
              )
              .await,
              Ok(true)
            )
          {
            tracing::info!(
                build_id = %build.id, drv = %build.drv_path,
                "Cached failure: skipping known-failing derivation"
            );
            if let Err(e) = repo::builds::start(&pool, build.id).await {
              tracing::warn!(build_id = %build.id, "Failed to start cached-failure build: {e}");
            }
            if let Err(e) = repo::builds::complete(
              &pool,
              build.id,
              BuildStatus::CachedFailure,
              None,
              None,
              Some("Build skipped: derivation is in failed paths cache"),
            )
            .await
            {
              tracing::warn!(build_id = %build.id, "Failed to complete cached-failure build: {e}");
            }
            continue;
          }

          // Dependency-aware scheduling: skip if deps not met
          match repo::build_dependencies::all_deps_completed(&pool, build.id)
            .await
          {
            Ok(true) => {},
            Ok(false) => {
              tracing::debug!(
                  build_id = %build.id,
                  "Build waiting for dependencies"
              );
              continue;
            },
            Err(e) => {
              tracing::error!(
                  build_id = %build.id,
                  "Failed to check build deps: {e}"
              );
              continue;
            },
          }

          // Unsupported system timeout: abort builds with no available builders
          if let Some(timeout) = unsupported_timeout
            && let Some(system) = &build.system
          {
            match repo::remote_builders::find_for_system(
              &pool,
              system,
              &scheduling_strategy,
            )
            .await
            {
              Ok(builders) if builders.is_empty() => {
                let timeout_at = build.created_at + timeout;
                if chrono::Utc::now() > timeout_at {
                  tracing::info!(
                    build_id = %build.id,
                    system = %system,
                    timeout = ?timeout,
                    "Aborting build: no builder available for system type"
                  );

                  if let Err(e) = repo::builds::start(&pool, build.id).await {
                    tracing::warn!(build_id = %build.id, "Failed to start unsupported build: {e}");
                  }

                  if let Err(e) = repo::builds::complete(
                    &pool,
                    build.id,
                    BuildStatus::UnsupportedSystem,
                    None,
                    None,
                    Some("No builder available for system type"),
                  )
                  .await
                  {
                    tracing::warn!(build_id = %build.id, "Failed to complete unsupported build: {e}");
                  }

                  continue;
                }
              },
              Ok(_) => {}, // Builders available, proceed normally
              Err(e) => {
                tracing::error!(
                  build_id = %build.id,
                  "Failed to check builders for unsupported system: {e}"
                );
                continue;
              },
            }
          }

          // One-at-a-time scheduling: check if jobset allows concurrent builds
          // First, get the evaluation to find the jobset
          let eval =
            match repo::evaluations::get(&pool, build.evaluation_id).await {
              Ok(eval) => eval,
              Err(e) => {
                tracing::error!(
                    build_id = %build.id,
                    evaluation_id = %build.evaluation_id,
                    "Failed to get evaluation for one-at-a-time check: {e}"
                );
                continue;
              },
            };

          let jobset = match repo::jobsets::get(&pool, eval.jobset_id).await {
            Ok(jobset) => jobset,
            Err(e) => {
              tracing::error!(
                  build_id = %build.id,
                  jobset_id = %eval.jobset_id,
                  "Failed to get jobset for one-at-a-time check: {e}"
              );
              continue;
            },
          };

          if jobset.state == JobsetState::OneAtATime {
            match repo::jobsets::has_running_builds(&pool, jobset.id).await {
              Ok(true) => {
                tracing::debug!(
                    build_id = %build.id,
                    jobset = %jobset.name,
                    "One-at-a-time: skipping, another build is running"
                );
                continue;
              },
              Ok(false) => {},
              Err(e) => {
                tracing::error!(
                    build_id = %build.id,
                    "Failed to check running builds: {e}"
                );
                continue;
              },
            }
          }

          worker_pool.dispatch(build);
        }
      },
      Err(e) => {
        if strict_errors {
          return Err(anyhow::anyhow!("Failed to fetch pending builds: {e}"));
        }
        tracing::error!("Failed to fetch pending builds: {e}");
      },
    }
    // Wake on NOTIFY or fall back to regular poll interval
    let _ = tokio::time::timeout(poll_interval, wakeup.notified()).await;
  }
}
