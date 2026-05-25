use std::{sync::Arc, time::Duration};

use clap::Parser;
use fc_common::{
  config::{Config, GcConfig, HotConfig},
  database::Database,
  gc_roots,
  repo,
};
use fc_queue_runner::worker::{ActiveBuilds, WorkerPool};
use tokio::sync::RwLock;

#[derive(Parser)]
#[command(name = "fc-queue-runner")]
#[command(about = "CI Queue Runner - Build dispatch and execution")]
struct Cli {
  #[arg(short, long)]
  workers: Option<usize>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  let cli = Cli::parse();

  let config = Config::load()?;
  fc_common::init_tracing(&config.tracing);

  tracing::info!("Starting CI Queue Runner");

  let hot_config = Arc::new(RwLock::new(HotConfig::from_config(&config)));

  let log_config = config.logs;
  let gc_config = config.gc;
  let gc_config_for_loop = gc_config.clone();
  let signing_config = config.signing;
  let cache_upload_config = config.cache_upload;
  let qr_config = config.queue_runner;

  let workers = cli.workers.unwrap_or(qr_config.workers);
  let strict_errors = qr_config.strict_errors;
  let failed_paths_cache = qr_config.failed_paths_cache;
  let work_dir = qr_config.work_dir;
  let unsupported_timeout = qr_config.unsupported_timeout;
  let alert_config = config.notifications.alerts.clone();

  // Ensure the work directory exists
  tokio::fs::create_dir_all(&work_dir).await?;

  // Clean up orphaned active logs from previous crashes
  cleanup_stale_logs(&log_config.log_dir).await;

  let db = Database::new(config.database).await?;

  let worker_pool = Arc::new(WorkerPool::new(
    db.pool().clone(),
    workers,
    work_dir.clone(),
    hot_config.clone(),
    log_config,
    gc_config,
    signing_config,
    cache_upload_config,
    alert_config,
  ));

  {
    let hot = hot_config.read().await;
    tracing::info!(
        workers = workers,
        poll_interval = ?hot.poll_interval,
        build_timeout = ?hot.build_timeout,
        work_dir = %work_dir.display(),
        "Queue runner configured"
    );
  }

  let worker_pool_for_drain = worker_pool.clone();

  let wakeup = Arc::new(tokio::sync::Notify::new());
  let listener_handle = fc_common::pg_notify::spawn_listener(
    db.pool(),
    &[fc_common::pg_notify::CHANNEL_BUILDS_CHANGED],
    wakeup.clone(),
  );

  let active_builds = worker_pool.active_builds().clone();

  tokio::select! {
      result = fc_queue_runner::runner_loop::run(db.pool().clone(), worker_pool, hot_config.clone(), wakeup, strict_errors, failed_paths_cache, unsupported_timeout) => {
          if let Err(e) = result {
              tracing::error!("Runner loop failed: {e}");
          }
      }
      () = gc_loop(gc_config_for_loop, db.pool().clone()) => {}
      () = failed_paths_cleanup_loop(db.pool().clone(), hot_config.clone(), failed_paths_cache) => {}
      () = cancel_checker_loop(db.pool().clone(), active_builds) => {}
      () = notification_retry_loop(db.pool().clone(), hot_config.clone()) => {}
      () = sighup_loop(hot_config.clone()) => {}
      () = shutdown_signal() => {
          tracing::info!("Shutdown signal received, draining in-flight builds...");
          worker_pool_for_drain.drain();
          worker_pool_for_drain.wait_for_drain().await;
          tracing::info!("All in-flight builds completed");
      }
  }

  listener_handle.abort();
  let _ = listener_handle.await;

  tracing::info!("Queue runner shutting down, closing database pool");
  db.close().await;

  Ok(())
}

async fn cleanup_stale_logs(log_dir: &std::path::Path) {
  if let Ok(mut entries) = tokio::fs::read_dir(log_dir).await {
    while let Ok(Some(entry)) = entries.next_entry().await {
      if entry.file_name().to_string_lossy().ends_with(".active.log") {
        let _ = tokio::fs::remove_file(entry.path()).await;
        tracing::info!("Removed stale active log: {}", entry.path().display());
      }
    }
  }
}

async fn gc_loop(gc_config: GcConfig, pool: sqlx::PgPool) {
  if !gc_config.enabled {
    return std::future::pending().await;
  }
  let interval = std::time::Duration::from_secs(gc_config.cleanup_interval);
  let max_age = std::time::Duration::from_secs(gc_config.max_age_days * 86400);

  loop {
    tokio::time::sleep(interval).await;

    let pinned = match repo::builds::list_pinned_ids(&pool).await {
      Ok(ids) => ids,
      Err(e) => {
        tracing::warn!("Failed to fetch pinned build IDs for GC: {e}");
        std::collections::HashSet::new()
      },
    };

    match gc_roots::cleanup_old_roots(&gc_config.gc_roots_dir, max_age, &pinned)
    {
      Ok(count) if count > 0 => {
        tracing::info!(count, "Cleaned up old GC roots");
        // Optionally run nix-collect-garbage
        match tokio::process::Command::new("nix-collect-garbage")
          .output()
          .await
        {
          Ok(output) if output.status.success() => {
            tracing::info!("nix-collect-garbage completed");
          },
          Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!("nix-collect-garbage failed: {stderr}");
          },
          Err(e) => {
            tracing::warn!("Failed to run nix-collect-garbage: {e}");
          },
        }
      },
      Ok(_) => {},
      Err(e) => {
        tracing::error!("GC cleanup failed: {e}");
      },
    }
  }
}

async fn failed_paths_cleanup_loop(
  pool: sqlx::PgPool,
  hot_config: Arc<RwLock<HotConfig>>,
  enabled: bool,
) {
  if !enabled {
    return std::future::pending().await;
  }

  let interval = std::time::Duration::from_hours(1);
  loop {
    tokio::time::sleep(interval).await;
    let ttl = hot_config.read().await.failed_paths_ttl;
    match fc_common::repo::failed_paths_cache::cleanup_expired(&pool, ttl).await
    {
      Ok(count) if count > 0 => {
        tracing::info!(count, "Cleaned up expired failed paths cache entries");
      },
      Ok(_) => {},
      Err(e) => {
        tracing::error!("Failed paths cache cleanup failed: {e}");
      },
    }
  }
}

async fn cancel_checker_loop(pool: sqlx::PgPool, active_builds: ActiveBuilds) {
  let interval = Duration::from_secs(2);
  loop {
    tokio::time::sleep(interval).await;

    let build_ids: Vec<uuid::Uuid> =
      active_builds.iter().map(|entry| *entry.key()).collect();

    if build_ids.is_empty() {
      continue;
    }

    match repo::builds::get_cancelled_among(&pool, &build_ids).await {
      Ok(cancelled_ids) => {
        for id in cancelled_ids {
          if let Some((_, token)) = active_builds.remove(&id) {
            tracing::info!(build_id = %id, "Triggering cancellation for running build");
            token.cancel();
          }
        }
      },
      Err(e) => {
        tracing::warn!("Failed to check for cancelled builds: {e}");
      },
    }
  }
}

async fn sighup_loop(hot_config: Arc<RwLock<HotConfig>>) {
  #[cfg(unix)]
  {
    use tokio::signal::unix::SignalKind;
    let mut sighup = match tokio::signal::unix::signal(SignalKind::hangup()) {
      Ok(s) => s,
      Err(e) => {
        tracing::warn!("Failed to install SIGHUP handler: {e}");
        return std::future::pending().await;
      },
    };
    loop {
      sighup.recv().await;
      tracing::info!("SIGHUP received, reloading configuration");
      match Config::load() {
        Ok(new_config) => {
          let new_hot = HotConfig::from_config(&new_config);
          tracing::info!(
            poll_interval = ?new_hot.poll_interval,
            build_timeout = ?new_hot.build_timeout,
            "Hot config reloaded (workers and database settings require restart)"
          );
          *hot_config.write().await = new_hot;
        },
        Err(e) => {
          tracing::error!("Failed to reload config on SIGHUP: {e}");
        },
      }
    }
  }
  #[cfg(not(unix))]
  std::future::pending().await
}

async fn notification_retry_loop(
  pool: sqlx::PgPool,
  hot_config: Arc<RwLock<HotConfig>>,
) {
  let (enable_retry_queue, poll_interval, retention_days) = {
    let hot = hot_config.read().await;
    (
      hot.notifications_config.enable_retry_queue,
      std::time::Duration::from_secs(
        hot.notifications_config.retry_poll_interval,
      ),
      hot.notifications_config.retention_days,
    )
  };

  if !enable_retry_queue {
    return std::future::pending().await;
  }

  let cleanup_pool = pool.clone();
  tokio::spawn(async move {
    let cleanup_interval = std::time::Duration::from_hours(1);
    loop {
      tokio::time::sleep(cleanup_interval).await;
      match repo::notification_tasks::cleanup_old_tasks(
        &cleanup_pool,
        retention_days,
      )
      .await
      {
        Ok(count) if count > 0 => {
          tracing::info!(count, "Cleaned up old notification tasks");
        },
        Ok(_) => {},
        Err(e) => {
          tracing::error!("Notification task cleanup failed: {e}");
        },
      }
    }
  });

  loop {
    tokio::time::sleep(poll_interval).await;

    let tasks = match repo::notification_tasks::list_pending(&pool, 10).await {
      Ok(t) => t,
      Err(e) => {
        tracing::warn!("Failed to fetch pending notification tasks: {e}");
        continue;
      },
    };

    for task in tasks {
      if let Err(e) =
        repo::notification_tasks::mark_running(&pool, task.id).await
      {
        tracing::warn!(task_id = %task.id, "Failed to mark task as running: {e}");
        continue;
      }

      match fc_common::notifications::process_notification_task(&task).await {
        Ok(()) => {
          if let Err(e) =
            repo::notification_tasks::mark_completed(&pool, task.id).await
          {
            tracing::error!(task_id = %task.id, "Failed to mark task as completed: {e}");
          } else {
            tracing::info!(
                task_id = %task.id,
                notification_type = %task.notification_type,
                attempts = task.attempts + 1,
                "Notification task completed"
            );
          }
        },
        Err(err) => {
          if let Err(e) = repo::notification_tasks::mark_failed_and_retry(
            &pool, task.id, &err,
          )
          .await
          {
            tracing::error!(task_id = %task.id, "Failed to update task status: {e}");
          } else {
            let status_after = if task.attempts + 1 >= task.max_attempts {
              "failed permanently"
            } else {
              "scheduled for retry"
            };
            tracing::warn!(
                task_id = %task.id,
                notification_type = %task.notification_type,
                attempts = task.attempts + 1,
                max_attempts = task.max_attempts,
                error = %err,
                status = status_after,
                "Notification task failed"
            );
          }
        },
      }
    }
  }
}

async fn shutdown_signal() {
  let ctrl_c = async {
    tokio::signal::ctrl_c()
      .await
      .expect("failed to install Ctrl+C handler");
  };

  #[cfg(unix)]
  let terminate = async {
    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
      .expect("failed to install SIGTERM handler")
      .recv()
      .await;
  };

  #[cfg(not(unix))]
  let terminate = std::future::pending::<()>();

  tokio::select! {
      () = ctrl_c => {},
      () = terminate => {},
  }
}
