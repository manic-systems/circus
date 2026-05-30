use std::{sync::Arc, time::Duration};

use circus_common::{
  config::{Config, GcConfig, HotConfig},
  database::Database,
  gc_roots,
  repo,
};
use circus_queue_runner::{
  rpc::{AgentPool, server::ServerConfig as RpcServerConfig},
  worker::{ActiveBuilds, WorkerPool},
};
use clap::Parser;
use tokio::sync::RwLock;

#[derive(Parser)]
#[command(name = "circus-queue-runner")]
#[command(about = "CI Queue Runner - Build dispatch and execution")]
struct Cli {
  #[arg(short, long)]
  workers: Option<usize>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  let cli = Cli::parse();

  let config = Config::load()?;
  circus_common::init_tracing(&config.tracing);

  tracing::info!("Starting CI Queue Runner");

  let hot_config = Arc::new(RwLock::new(HotConfig::from_config(&config)));

  let log_config = config.logs;
  let gc_config = config.gc;
  let gc_config_for_loop = gc_config.clone();
  let signing_config = config.signing;
  let signing_config_for_rpc = signing_config.clone();
  let cache_upload_config = config.cache_upload;
  let cache_upload_for_rpc = cache_upload_config.clone();
  let qr_config = config.queue_runner;
  let nix_store_dir = config.nix.store_dir;

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

  // Crash recovery for builder_sessions.connected: rows from any
  // previous runner instance are stale once we boot. The RPC accept
  // loop will flip each agent back to connected on register.
  match repo::builder_sessions::reset_all_connected(db.pool()).await {
    Ok(n) if n > 0 => {
      tracing::info!(reset = n, "cleared stale builder_sessions.connected");
    },
    Ok(_) => {},
    Err(e) => tracing::warn!("could not reset builder_sessions.connected: {e}"),
  }

  let signing_enabled = signing_config.enabled;
  let signing_key_file = signing_config.key_file.clone();

  // Agent pool must be constructed BEFORE the worker pool so the
  // scheduler can hand work to connected agents. The RPC server thread
  // is started further down; agents that register before the listener is
  // up simply reconnect via their supervisor loop.
  let agent_pool = AgentPool::new();
  let heartbeat_ttl = qr_config.rpc.as_ref().map_or_else(
    || std::time::Duration::from_mins(1),
    |c| std::time::Duration::from_secs(c.heartbeat_ttl_secs),
  );

  let worker_pool = Arc::new(WorkerPool::new(
    db.pool().clone(),
    workers,
    work_dir.clone(),
    nix_store_dir,
    Arc::clone(&hot_config),
    log_config,
    gc_config,
    signing_config,
    cache_upload_config,
    alert_config,
    Arc::clone(&agent_pool),
    heartbeat_ttl,
  ));

  {
    let hot = hot_config.read().await;
    let nc = &hot.notifications_config;
    tracing::info!(
        workers = workers,
        poll_interval = ?hot.poll_interval,
        build_timeout = ?hot.build_timeout,
        work_dir = %work_dir.display(),
        enable_retry_queue = nc.enable_retry_queue,
        webhook_url_set = nc.webhook_url.is_some(),
        github_token_set = nc.github_token.is_some(),
        slack_set = nc.slack.is_some(),
        email_set = nc.email.is_some(),
        signing_enabled = signing_enabled,
        signing_key_file = ?signing_key_file,
        signing_key_file_exists = signing_key_file.as_ref().is_some_and(|p| p.exists()),
        "Queue runner configured"
    );
  }

  let worker_pool_for_drain = Arc::clone(&worker_pool);

  let wakeup = Arc::new(tokio::sync::Notify::new());
  let listener_handle = circus_common::pg_notify::spawn_listener(
    db.pool(),
    &[circus_common::pg_notify::CHANNEL_BUILDS_CHANGED],
    Arc::clone(&wakeup),
  );

  let active_builds = Arc::clone(worker_pool.active_builds());

  // capnp-rpc lives in its own thread because its capabilities are
  // !Send; the rest of the queue-runner stays on the multi-threaded
  // runtime. The AgentPool (constructed above and shared with the
  // worker pool) bridges the boundary via per-agent channels.
  if let Some(rpc_cfg) = qr_config.rpc.clone() {
    spawn_rpc_thread(
      rpc_cfg,
      cache_upload_for_rpc,
      signing_config_for_rpc,
      Arc::clone(&agent_pool),
      db.pool().clone(),
    );
  } else {
    tracing::info!(
      "[queue_runner.rpc] not set; agent path disabled, falling back to SSH \
       dispatch only"
    );
  }

  tokio::select! {
      result = circus_queue_runner::runner_loop::run(db.pool().clone(), worker_pool, Arc::clone(&hot_config), wakeup, strict_errors, failed_paths_cache, unsupported_timeout) => {
          if let Err(e) = result {
              tracing::error!("Runner loop failed: {e}");
          }
      }
      () = gc_loop(gc_config_for_loop, db.pool().clone()) => {}
      () = failed_paths_cleanup_loop(db.pool().clone(), Arc::clone(&hot_config), failed_paths_cache) => {}
      () = cancel_checker_loop(db.pool().clone(), active_builds) => {}
      () = notification_retry_loop(db.pool().clone(), Arc::clone(&hot_config)) => {}
      () = sighup_loop(Arc::clone(&hot_config)) => {}
      () = heartbeat_loop(db.pool().clone(), qr_config.poll_interval) => {}
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

/// Spawn the capnp-rpc agent listener on its own current-thread runtime.
///
/// capnp-rpc capabilities are reference-counted with `Rc`, so all task
/// driving the RPC system must live on a single thread. We isolate that
/// to one OS thread; the main multi-threaded runtime is untouched. Cross
/// the boundary via `Arc<AgentPool>` (channels inside).
fn spawn_rpc_thread(
  cfg: circus_common::config::RpcConfig,
  cache_cfg: circus_common::config::CacheUploadConfig,
  signing_cfg: circus_common::config::SigningConfig,
  pool: Arc<AgentPool>,
  db_pool: sqlx::PgPool,
) {
  std::thread::Builder::new()
    .name("circus-rpc".into())
    .spawn(move || {
      let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
      {
        Ok(r) => r,
        Err(e) => {
          tracing::error!("rpc runtime build failed: {e}");
          return;
        },
      };
      let local = tokio::task::LocalSet::new();
      let server_cfg = match RpcServerConfig::from_user(&cfg) {
        Ok(c) => {
          c.with_presigner_from(&cache_cfg).with_signing_key(
            signing_cfg
              .enabled
              .then_some(signing_cfg.key_file)
              .flatten(),
          )
        },
        Err(e) => {
          tracing::error!("rpc config error: {e}");
          return;
        },
      };
      rt.block_on(local.run_until(async move {
        if let Err(e) =
          circus_queue_runner::rpc::serve(server_cfg, pool, db_pool).await
        {
          tracing::error!("rpc server ended: {e}");
        }
      }));
    })
    .map_or_else(
      |e| {
        tracing::error!("failed to spawn rpc thread: {e}");
      },
      |_| {},
    );
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

  #[expect(clippy::infinite_loop, reason = "intentional background GC loop")]
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
  #[expect(
    clippy::infinite_loop,
    reason = "intentional background cleanup loop"
  )]
  loop {
    tokio::time::sleep(interval).await;
    let ttl = hot_config.read().await.failed_paths_ttl;
    match circus_common::repo::failed_paths_cache::cleanup_expired(&pool, ttl)
      .await
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
  #[expect(
    clippy::infinite_loop,
    reason = "intentional background cancel check loop"
  )]
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

/// Write a service heartbeat on every poll tick so the server's /health
/// endpoint can report queue-runner liveness.
async fn heartbeat_loop(pool: sqlx::PgPool, poll_interval_seconds: u64) {
  let interval = std::time::Duration::from_secs(poll_interval_seconds.max(1));
  // Emit one immediately so /health doesn't return "never reported" during
  // the first poll interval after startup.
  if let Err(e) = circus_common::service_heartbeat::record(
    &pool,
    circus_common::service_heartbeat::SERVICE_QUEUE_RUNNER,
    u32::try_from(poll_interval_seconds.min(u64::from(u32::MAX)))
      .unwrap_or(u32::MAX),
    Some(env!("CARGO_PKG_VERSION")),
  )
  .await
  {
    tracing::warn!("initial queue-runner heartbeat failed: {e}");
  }

  #[expect(
    clippy::infinite_loop,
    reason = "intentional background heartbeat loop"
  )]
  loop {
    tokio::time::sleep(interval).await;
    if let Err(e) = circus_common::service_heartbeat::record(
      &pool,
      circus_common::service_heartbeat::SERVICE_QUEUE_RUNNER,
      u32::try_from(poll_interval_seconds.min(u64::from(u32::MAX)))
        .unwrap_or(u32::MAX),
      Some(env!("CARGO_PKG_VERSION")),
    )
    .await
    {
      tracing::warn!("queue-runner heartbeat failed: {e}");
    }
  }
}

async fn sighup_loop(hot_config: Arc<RwLock<HotConfig>>) {
  #[expect(clippy::infinite_loop, reason = "intentional SIGHUP handler loop")]
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

  #[expect(
    clippy::infinite_loop,
    reason = "intentional notification retry loop"
  )]
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

      match circus_common::notifications::process_notification_task(&task).await
      {
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
  #[expect(
    clippy::expect_used,
    reason = "signal handler install failure indicates a broken runtime; \
              panicking is appropriate"
  )]
  let ctrl_c = async {
    tokio::signal::ctrl_c()
      .await
      .expect("failed to install Ctrl+C handler");
  };

  #[cfg(unix)]
  #[expect(
    clippy::expect_used,
    reason = "signal handler install failure indicates a broken runtime; \
              panicking is appropriate"
  )]
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
