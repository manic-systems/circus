use std::{path::PathBuf, sync::Arc, time::Duration};

use dashmap::DashMap;
use fc_common::{
  alerts::AlertManager,
  config::{
    AlertConfig,
    CacheUploadConfig,
    GcConfig,
    HotConfig,
    LogConfig,
    NotificationsConfig,
    SigningConfig,
  },
  gc_roots::GcRoots,
  log_storage::LogStorage,
  models::{
    Build,
    BuildStatus,
    CreateBuildProduct,
    CreateBuildStep,
    metric_names,
    metric_units,
  },
  repo,
};
use sqlx::PgPool;
use tokio::sync::{RwLock, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

pub type ActiveBuilds = Arc<DashMap<Uuid, CancellationToken>>;

pub struct WorkerPool {
  semaphore:           Arc<Semaphore>,
  upload_semaphore:    Arc<Semaphore>,
  worker_count:        usize,
  pool:                PgPool,
  work_dir:            Arc<PathBuf>,
  hot_config:          Arc<RwLock<HotConfig>>,
  log_config:          Arc<LogConfig>,
  gc_config:           Arc<GcConfig>,
  signing_config:      Arc<SigningConfig>,
  cache_upload_config: Arc<CacheUploadConfig>,
  alert_manager:       Arc<Option<AlertManager>>,
  psi_cache:           Arc<crate::psi::PsiCache>,
  drain_token:         CancellationToken,
  active_builds:       ActiveBuilds,
}

impl WorkerPool {
  #[allow(clippy::too_many_arguments)]
  #[must_use]
  pub fn new(
    db_pool: PgPool,
    workers: usize,
    work_dir: PathBuf,
    hot_config: Arc<RwLock<HotConfig>>,
    log_config: LogConfig,
    gc_config: GcConfig,
    signing_config: SigningConfig,
    cache_upload_config: CacheUploadConfig,
    alert_config: Option<AlertConfig>,
  ) -> Self {
    let alert_manager = alert_config.map(AlertManager::new);
    let upload_concurrency = cache_upload_config.upload_concurrency.max(1);
    Self {
      semaphore: Arc::new(Semaphore::new(workers)),
      upload_semaphore: Arc::new(Semaphore::new(upload_concurrency)),
      worker_count: workers,
      pool: db_pool,
      work_dir: Arc::new(work_dir),
      hot_config,
      log_config: Arc::new(log_config),
      gc_config: Arc::new(gc_config),
      signing_config: Arc::new(signing_config),
      cache_upload_config: Arc::new(cache_upload_config),
      alert_manager: Arc::new(alert_manager),
      psi_cache: crate::psi::PsiCache::new(),
      drain_token: CancellationToken::new(),
      active_builds: Arc::new(DashMap::new()),
    }
  }

  /// Signal all workers to stop accepting new builds. In-flight builds will
  /// finish.
  pub fn drain(&self) {
    self.drain_token.cancel();
  }

  /// Wait until all in-flight builds complete (semaphore fully available).
  pub async fn wait_for_drain(&self) {
    // Acquire all permits = all workers idle
    let workers = self.worker_count;
    let build_timeout = self.hot_config.read().await.build_timeout;
    let _ = tokio::time::timeout(
      Duration::from_secs(build_timeout.as_secs() + 60),
      async {
        for _ in 0..workers {
          if let Ok(permit) = self.semaphore.acquire().await {
            permit.forget(); // don't release back
          }
        }
      },
    )
    .await;
  }

  #[must_use]
  pub const fn worker_count(&self) -> usize {
    self.worker_count
  }

  #[must_use]
  pub const fn active_builds(&self) -> &ActiveBuilds {
    &self.active_builds
  }

  #[tracing::instrument(skip(self, build), fields(build_id = %build.id, job = %build.job_name))]
  pub fn dispatch(&self, build: Build) {
    if self.drain_token.is_cancelled() {
      tracing::info!(build_id = %build.id, "Drain in progress, not dispatching");
      return;
    }

    let semaphore = self.semaphore.clone();
    let upload_semaphore = self.upload_semaphore.clone();
    let pool = self.pool.clone();
    let work_dir = self.work_dir.clone();
    let hot_config = self.hot_config.clone();
    let log_config = self.log_config.clone();
    let gc_config = self.gc_config.clone();
    let signing_config = self.signing_config.clone();
    let cache_upload_config = self.cache_upload_config.clone();
    let alert_manager = self.alert_manager.clone();
    let psi_cache = self.psi_cache.clone();
    let active_builds = self.active_builds.clone();
    let cancel_token = CancellationToken::new();
    let build_id = build.id;

    active_builds.insert(build_id, cancel_token.clone());

    tokio::spawn(async move {
      let result = async {
        let Ok(_permit) = semaphore.acquire().await else {
          return;
        };

        let (
          timeout,
          notifications_config,
          scheduling_strategy,
          psi_threshold,
          psi_check_timeout,
        ) = {
          let hot = hot_config.read().await;
          (
            hot.build_timeout,
            hot.notifications_config.clone(),
            hot.scheduling_strategy.clone(),
            hot.psi_threshold,
            hot.psi_check_timeout,
          )
        };

        if let Err(e) = run_build(
          &pool,
          &build,
          &work_dir,
          timeout,
          &log_config,
          &gc_config,
          &notifications_config,
          &signing_config,
          &cache_upload_config,
          &alert_manager,
          upload_semaphore.clone(),
          scheduling_strategy,
          psi_threshold,
          psi_check_timeout,
          psi_cache.clone(),
        )
        .await
        {
          tracing::error!(build_id = %build.id, "Build dispatch failed: {e}");
        }
      };

      tokio::select! {
        () = result => {}
        () = cancel_token.cancelled() => {
          tracing::info!(build_id = %build_id, "Build cancelled, aborting");
        }
      }

      active_builds.remove(&build_id);
    });
  }
}

/// Query nix path-info for narHash and narSize of an output path.
async fn get_path_info(output_path: &str) -> Option<(String, i64)> {
  let output = tokio::process::Command::new("nix")
    .args(["path-info", "--json", output_path])
    .output()
    .await
    .ok()?;

  if !output.status.success() {
    return None;
  }

  let stdout = String::from_utf8_lossy(&output.stdout);
  let parsed: serde_json::Value = serde_json::from_str(&stdout).ok()?;

  let entry = parsed.as_array()?.first()?;
  let nar_hash = entry.get("narHash")?.as_str()?.to_string();
  let nar_size = entry.get("narSize")?.as_i64()?;

  Some((nar_hash, nar_size))
}

/// Look up the project that owns a build (build -> evaluation -> jobset ->
/// project).
async fn get_project_for_build(
  pool: &PgPool,
  build: &Build,
) -> Option<(fc_common::models::Project, String)> {
  let eval = repo::evaluations::get(pool, build.evaluation_id)
    .await
    .ok()?;
  let jobset = repo::jobsets::get(pool, eval.jobset_id).await.ok()?;
  let project = repo::projects::get(pool, jobset.project_id).await.ok()?;
  Some((project, eval.commit_hash))
}

async fn dispatch_build_finished_notification(
  pool: &PgPool,
  build: &Build,
  notifications_config: &NotificationsConfig,
) {
  if let Some((project, commit_hash)) = get_project_for_build(pool, build).await
  {
    fc_common::notifications::dispatch_build_finished(
      Some(pool),
      build,
      &project,
      &commit_hash,
      notifications_config,
    )
    .await;
  }
}

/// Sign nix store outputs using the configured signing key.
async fn sign_outputs(
  output_paths: &[String],
  signing_config: &SigningConfig,
) -> bool {
  let key_file = match &signing_config.key_file {
    Some(kf) if signing_config.enabled && kf.exists() => kf,
    _ => return false,
  };

  for output_path in output_paths {
    let result = tokio::process::Command::new("nix")
      .args([
        "store",
        "sign",
        "--key-file",
        &key_file.to_string_lossy(),
        output_path,
      ])
      .output()
      .await;

    match result {
      Ok(o) if o.status.success() => {
        tracing::debug!(output = output_path, "Signed store path");
      },
      Ok(o) => {
        let stderr = String::from_utf8_lossy(&o.stderr);
        tracing::warn!(output = output_path, "Failed to sign: {stderr}");
      },
      Err(e) => {
        tracing::warn!(
          output = output_path,
          "Failed to run nix store sign: {e}"
        );
      },
    }
  }
  true
}

/// Push output paths to an external binary cache via `nix copy`. Returns
/// the list of paths that exhausted their retry budget. An empty Vec
/// means every path made it.
async fn push_to_cache(
  output_paths: &[String],
  store_uri: &str,
  s3_config: Option<&fc_common::config::S3CacheConfig>,
  semaphore: Arc<Semaphore>,
  max_retries: u32,
) -> Vec<String> {
  let full_store_uri = if store_uri.starts_with("s3://") {
    build_s3_store_uri(store_uri, s3_config)
  } else {
    store_uri.to_string()
  };

  let mut failed = Vec::new();
  for path in output_paths {
    let _permit = semaphore.acquire().await;
    let mut success = false;
    for attempt in 0..=max_retries {
      let result = tokio::process::Command::new("nix")
        .args(["copy", "--to", &full_store_uri, path])
        .kill_on_drop(true)
        .output()
        .await;
      match result {
        Ok(o) if o.status.success() => {
          tracing::debug!(
            output = path,
            store = store_uri,
            "Pushed to binary cache"
          );
          success = true;
          break;
        },
        Ok(o) => {
          let stderr = String::from_utf8_lossy(&o.stderr);
          if attempt < max_retries {
            tracing::warn!(
              output = path,
              attempt = attempt + 1,
              max_retries,
              "Push to cache failed, retrying: {stderr}"
            );
            tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
          } else {
            tracing::error!(
              output = path,
              "Failed to push to cache after {max_retries} retries: {stderr}"
            );
          }
        },
        Err(e) => {
          if attempt < max_retries {
            tracing::warn!(
              output = path,
              attempt = attempt + 1,
              "nix copy error, retrying: {e}"
            );
            tokio::time::sleep(Duration::from_secs(2u64.pow(attempt))).await;
          } else {
            tracing::error!(output = path, "nix copy permanently failed: {e}");
          }
        },
      }
    }
    if !success {
      failed.push(path.clone());
    }
  }
  failed
}

/// Build S3 store URI with configuration options.
/// Nix S3 URIs support query parameters for configuration:
/// <s3://bucket?region=us-east-1&endpoint=https://minio.example.com>
fn build_s3_store_uri(
  base_uri: &str,
  config: Option<&fc_common::config::S3CacheConfig>,
) -> String {
  let Some(cfg) = config else {
    return base_uri.to_string();
  };

  let mut params: Vec<(&str, &str)> = Vec::new();

  if let Some(region) = &cfg.region {
    params.push(("region", region));
  }

  if let Some(endpoint) = &cfg.endpoint_url {
    params.push(("endpoint", endpoint));
  }

  if cfg.use_path_style {
    params.push(("use-path-style", "true"));
  }

  if params.is_empty() {
    return base_uri.to_string();
  }

  let query = params
    .iter()
    .map(|(k, v)| {
      format!("{}={}", urlencoding::encode(k), urlencoding::encode(v))
    })
    .collect::<Vec<_>>()
    .join("&");

  format!("{base_uri}?{query}")
}

/// Try to run the build on a remote builder if one is available for the build's
/// system.
#[allow(clippy::too_many_arguments)]
async fn try_remote_build(
  pool: &PgPool,
  build: &Build,
  work_dir: &std::path::Path,
  timeout: Duration,
  live_log_path: Option<&std::path::Path>,
  strategy: &fc_common::config::BuilderSchedulingStrategy,
  psi_threshold: Option<f64>,
  psi_check_timeout: Duration,
  psi_cache: &crate::psi::PsiCache,
) -> Option<crate::builder::BuildResult> {
  let system = build.system.as_deref()?;

  let builders = repo::remote_builders::find_for_system(pool, system, strategy)
    .await
    .ok()?;

  for builder in &builders {
    if let Some(threshold) = psi_threshold
      && let Some(snap) =
        crate::psi::read_cached(psi_cache, &builder.ssh_uri, psi_check_timeout)
          .await
      && snap.exceeds(threshold)
    {
      tracing::debug!(
        build_id = %build.id,
        builder = %builder.name,
        cpu_avg10 = snap.cpu_avg10,
        memory_avg10 = snap.memory_avg10,
        io_avg10 = snap.io_avg10,
        threshold,
        "PSI: builder overloaded, skipping"
      );
      continue;
    }
    tracing::info!(
        build_id = %build.id,
        builder = %builder.name,
        "Attempting remote build on {}",
        builder.ssh_uri,
    );

    // Set builder_id
    if let Err(e) = repo::builds::set_builder(pool, build.id, builder.id).await
    {
      tracing::warn!(build_id = %build.id, builder = %builder.name, "Failed to set builder_id: {e}");
    }

    // Build remotely via --store
    let store_uri = format!("ssh://{}", builder.ssh_uri);
    let result = crate::builder::run_nix_build_remote(
      &build.drv_path,
      work_dir,
      timeout,
      &store_uri,
      builder.ssh_key_file.as_deref(),
      live_log_path,
    )
    .await;

    match result {
      Ok(r) => {
        if let Err(e) =
          repo::remote_builders::record_success(pool, builder.id).await
        {
          tracing::warn!(builder = %builder.name, "Failed to record builder success: {e}");
        }
        return Some(r);
      },
      Err(e) => {
        tracing::warn!(
            build_id = %build.id,
            builder = %builder.name,
            "Remote build failed: {e}, trying next builder"
        );
        if let Err(e) =
          repo::remote_builders::record_failure(pool, builder.id).await
        {
          tracing::warn!(builder = %builder.name, "Failed to record builder failure: {e}");
        }
      },
    }
  }

  None
}

async fn collect_metrics_and_alert(
  pool: &PgPool,
  build: &Build,
  output_paths: &[String],
  alert_manager: &Option<AlertManager>,
) {
  if let (Some(started), Some(completed)) =
    (build.started_at, build.completed_at)
  {
    let duration = completed.signed_duration_since(started);
    let duration_secs = duration.num_seconds() as f64;

    if let Err(e) = repo::build_metrics::upsert(
      pool,
      build.id,
      metric_names::BUILD_DURATION_SECONDS,
      duration_secs,
      metric_units::SECONDS,
    )
    .await
    {
      tracing::warn!("Failed to save build duration metric: {}", e);
    }
  }

  for path in output_paths {
    if let Ok(meta) = tokio::fs::metadata(path).await {
      let size = meta.len();
      if let Err(e) = repo::build_metrics::upsert(
        pool,
        build.id,
        metric_names::OUTPUT_SIZE_BYTES,
        size as f64,
        metric_units::BYTES,
      )
      .await
      {
        tracing::warn!("Failed to save output size metric: {}", e);
        continue;
      }
      break;
    }
  }

  let Some(manager) = alert_manager else {
    return;
  };

  if manager.is_enabled()
    && let Ok(evaluation) =
      repo::evaluations::get(pool, build.evaluation_id).await
    && let Ok(jobset) = repo::jobsets::get(pool, evaluation.jobset_id).await
  {
    manager
      .check_and_alert(pool, Some(jobset.project_id), Some(jobset.id))
      .await;
  }
}

#[tracing::instrument(skip(pool, build, work_dir, log_config, gc_config, notifications_config, signing_config, cache_upload_config, upload_semaphore, scheduling_strategy), fields(build_id = %build.id, job = %build.job_name))]
#[allow(clippy::too_many_arguments)]
async fn run_build(
  pool: &PgPool,
  build: &Build,
  work_dir: &std::path::Path,
  timeout: Duration,
  log_config: &LogConfig,
  gc_config: &GcConfig,
  notifications_config: &NotificationsConfig,
  signing_config: &SigningConfig,
  cache_upload_config: &CacheUploadConfig,
  alert_manager: &Option<AlertManager>,
  upload_semaphore: Arc<Semaphore>,
  scheduling_strategy: fc_common::config::BuilderSchedulingStrategy,
  psi_threshold: Option<f64>,
  psi_check_timeout: Duration,
  psi_cache: Arc<crate::psi::PsiCache>,
) -> anyhow::Result<()> {
  // Atomically claim the build
  let claimed = repo::builds::start(pool, build.id).await?;
  if claimed.is_none() {
    tracing::debug!(build_id = %build.id, "Build already claimed, skipping");
    return Ok(());
  }

  let claimed_build = claimed.unwrap(); // Safe: we checked is_some()

  // Dispatch build started notification
  if let Some((project, commit_hash)) =
    get_project_for_build(pool, &claimed_build).await
  {
    fc_common::notifications::dispatch_build_started(
      pool,
      &claimed_build,
      &project,
      &commit_hash,
      notifications_config,
    )
    .await;
  }

  tracing::info!(build_id = %build.id, job = %build.job_name, "Starting build");

  // Create a build step record
  let step = repo::build_steps::create(pool, CreateBuildStep {
    build_id:    build.id,
    step_number: 1,
    command:     format!(
      "nix build --no-link --print-out-paths {}",
      build.drv_path
    ),
  })
  .await?;

  // Set up live log path
  let live_log_path =
    log_config.log_dir.join(format!("{}.active.log", build.id));
  let _ = tokio::fs::create_dir_all(&log_config.log_dir).await;

  // Try remote build first, then fall back to local
  let result = if build.system.is_some() {
    match try_remote_build(
      pool,
      build,
      work_dir,
      timeout,
      Some(&live_log_path),
      &scheduling_strategy,
      psi_threshold,
      psi_check_timeout,
      &psi_cache,
    )
    .await
    {
      Some(r) => Ok(r),
      None => {
        // No remote builder available or all failed, build locally
        crate::builder::run_nix_build(
          &build.drv_path,
          work_dir,
          timeout,
          Some(&live_log_path),
        )
        .await
      },
    }
  } else {
    crate::builder::run_nix_build(
      &build.drv_path,
      work_dir,
      timeout,
      Some(&live_log_path),
    )
    .await
  };

  // Initialize log storage
  let log_storage = LogStorage::new(log_config.log_dir.clone()).ok();

  match result {
    Ok(build_result) => {
      // Complete the build step
      let exit_code = i32::from(!build_result.success);
      repo::build_steps::complete(
        pool,
        step.id,
        exit_code,
        Some(&build_result.stdout),
        Some(&build_result.stderr),
      )
      .await?;

      // Create sub-step records from parsed nix log
      for (i, sub_step) in build_result.sub_steps.iter().enumerate() {
        let sub = repo::build_steps::create(pool, CreateBuildStep {
          build_id:    build.id,
          step_number: (i as i32) + 2,
          command:     format!("nix build {}", sub_step.drv_path),
        })
        .await?;
        let sub_exit = i32::from(!sub_step.success);
        repo::build_steps::complete(pool, sub.id, sub_exit, None, None).await?;
      }

      // Write build log (rename active log to final)
      let log_path = if let Some(ref storage) = log_storage {
        let final_path = storage.log_path(&build.id);
        if live_log_path.exists() {
          if let Err(e) = tokio::fs::rename(&live_log_path, &final_path).await {
            tracing::warn!(build_id = %build.id, "Failed to rename build log: {e}");
          }
        } else {
          match storage.write_log(
            &build.id,
            &build_result.stdout,
            &build_result.stderr,
          ) {
            Ok(_) => {},
            Err(e) => {
              tracing::warn!(build_id = %build.id, "Failed to write build log: {e}");
            },
          }
        }
        Some(final_path.to_string_lossy().to_string())
      } else {
        None
      };

      if build_result.success {
        // Build a reverse lookup map: path -> output_name
        // The outputs JSON is a HashMap<String, String> where keys are output
        // names and values are store paths. We need to match paths to
        // names correctly.
        let path_to_name: std::collections::HashMap<String, String> = build
          .outputs
          .as_ref()
          .and_then(|v| v.as_object())
          .map(|obj| {
            obj
              .iter()
              .filter_map(|(name, path)| {
                path.as_str().map(|p| (p.to_string(), name.clone()))
              })
              .collect()
          })
          .unwrap_or_default();

        // Store build outputs in normalized table
        for (i, output_path) in build_result.output_paths.iter().enumerate() {
          let output_name =
            path_to_name.get(output_path).cloned().unwrap_or_else(|| {
              if i == 0 {
                "out".to_string()
              } else {
                format!("out{i}")
              }
            });

          if let Err(e) = repo::build_outputs::create(
            pool,
            build.id,
            &output_name,
            Some(output_path),
          )
          .await
          {
            tracing::warn!(
              build_id = %build.id,
              output_name = %output_name,
              "Failed to store build output: {e}"
            );
          }
        }

        // Register GC roots and create build products for each output
        for (i, output_path) in build_result.output_paths.iter().enumerate() {
          let output_name =
            path_to_name.get(output_path).cloned().unwrap_or_else(|| {
              if i == 0 {
                build.job_name.clone()
              } else {
                format!("{}-{i}", build.job_name)
              }
            });

          // Register GC root
          let mut gc_root_path = None;
          if let Ok(gc_roots) =
            GcRoots::new(gc_config.gc_roots_dir.clone(), gc_config.enabled)
          {
            let gc_id = if i == 0 {
              build.id
            } else {
              uuid::Uuid::new_v4()
            };
            match gc_roots.register(&gc_id, output_path) {
              Ok(Some(link_path)) => {
                gc_root_path = Some(link_path.to_string_lossy().to_string());
              },
              Ok(None) => {},
              Err(e) => {
                tracing::warn!(build_id = %build.id, "Failed to register GC root: {e}");
              },
            }
          }

          // Get metadata from nix path-info
          let (sha256_hash, file_size) = match get_path_info(output_path).await
          {
            Some((hash, size)) => (Some(hash), Some(size)),
            None => (None, None),
          };

          let product =
            repo::build_products::create(pool, CreateBuildProduct {
              build_id: build.id,
              name: output_name,
              path: output_path.clone(),
              sha256_hash,
              file_size,
              content_type: None,
              is_directory: true,
            })
            .await?;

          // Update the build product with GC root path if registered
          if gc_root_path.is_some() {
            sqlx::query(
              "UPDATE build_products SET gc_root_path = $1 WHERE id = $2",
            )
            .bind(&gc_root_path)
            .bind(product.id)
            .execute(pool)
            .await?;
          }
        }

        // Sign outputs at build time
        if sign_outputs(&build_result.output_paths, signing_config).await
          && let Err(e) = repo::builds::mark_signed(pool, build.id).await
        {
          tracing::warn!(build_id = %build.id, "Failed to mark build as signed: {e}");
        }

        // Push to external binary cache if configured
        let mut upload_failed_paths: Vec<String> = Vec::new();
        if cache_upload_config.enabled
          && let Some(ref store_uri) = cache_upload_config.store_uri
        {
          upload_failed_paths = push_to_cache(
            &build_result.output_paths,
            store_uri,
            cache_upload_config.s3.as_ref(),
            upload_semaphore.clone(),
            cache_upload_config.upload_max_retries,
          )
          .await;
        }

        if !upload_failed_paths.is_empty()
          && cache_upload_config.fail_build_on_upload_error
        {
          let msg = format!(
            "Cache upload failed for {} path(s): {}",
            upload_failed_paths.len(),
            upload_failed_paths.join(", "),
          );
          tracing::error!(build_id = %build.id, "{msg}");
          repo::builds::complete(
            pool,
            build.id,
            BuildStatus::Failed,
            log_path.as_deref(),
            None,
            Some(&msg),
          )
          .await?;
          let updated_build = repo::builds::get(pool, build.id).await?;
          dispatch_build_finished_notification(
            pool,
            &updated_build,
            notifications_config,
          )
          .await;
          return Ok(());
        }

        let primary_output = build_result
          .output_paths
          .first()
          .map(std::string::String::as_str);

        repo::builds::complete(
          pool,
          build.id,
          BuildStatus::Succeeded,
          log_path.as_deref(),
          primary_output,
          None,
        )
        .await?;

        collect_metrics_and_alert(
          pool,
          build,
          &build_result.output_paths,
          alert_manager,
        )
        .await;

        tracing::info!(build_id = %build.id, "Build completed successfully");
      } else {
        // Check if we should retry
        if build.retry_count < build.max_retries {
          tracing::info!(
              build_id = %build.id,
              retry = build.retry_count + 1,
              max = build.max_retries,
              "Build failed, scheduling retry"
          );
          // Clean up old build steps before retry
          sqlx::query("DELETE FROM build_steps WHERE build_id = $1")
            .bind(build.id)
            .execute(pool)
            .await?;
          sqlx::query(
            "UPDATE builds SET status = 'pending', started_at = NULL, \
             retry_count = retry_count + 1, completed_at = NULL WHERE id = $1",
          )
          .bind(build.id)
          .execute(pool)
          .await?;
          // Clean up live log
          let _ = tokio::fs::remove_file(&live_log_path).await;
          return Ok(());
        }

        let failure_status = build_result
          .exit_code
          .map_or(BuildStatus::Failed, BuildStatus::from_exit_code);
        repo::builds::complete(
          pool,
          build.id,
          failure_status,
          log_path.as_deref(),
          None,
          Some(&build_result.stderr),
        )
        .await?;

        if let Err(e) = repo::failed_paths_cache::insert(
          pool,
          &build.drv_path,
          failure_status,
          build.id,
        )
        .await
        {
          tracing::warn!(build_id = %build.id, "Failed to cache failed path: {e}");
        }

        tracing::warn!(build_id = %build.id, "Build failed: {:?}", failure_status);
      }
    },
    Err(e) => {
      let msg = e.to_string();

      // Write error log
      if let Some(ref storage) = log_storage
        && let Err(e) = storage.write_log(&build.id, "", &msg)
      {
        tracing::warn!(build_id = %build.id, "Failed to write error log: {e}");
      }
      // Clean up live log
      let _ = tokio::fs::remove_file(&live_log_path).await;

      repo::build_steps::complete(pool, step.id, 1, None, Some(&msg)).await?;
      repo::builds::complete(
        pool,
        build.id,
        BuildStatus::Failed,
        None,
        None,
        Some(&msg),
      )
      .await?;
      tracing::error!(build_id = %build.id, "Build error: {msg}");
    },
  }

  // Dispatch notifications after build completion
  let updated_build = repo::builds::get(pool, build.id).await?;
  if updated_build.status.is_finished() {
    dispatch_build_finished_notification(
      pool,
      &updated_build,
      notifications_config,
    )
    .await;

    // Auto-promote channels if all builds in the evaluation are done
    if updated_build.status.is_success()
      && let Ok(eval) = repo::evaluations::get(pool, build.evaluation_id).await
      && let Err(e) =
        repo::channels::auto_promote_if_complete(pool, eval.jobset_id, eval.id)
          .await
    {
      tracing::warn!(build_id = %build.id, "Failed to auto-promote channels: {e}");
    }
  }

  Ok(())
}

#[cfg(test)]
mod tests {
  use fc_common::config::S3CacheConfig;

  use super::*;

  #[test]
  fn test_build_s3_store_uri_no_config() {
    let result = build_s3_store_uri("s3://my-bucket", None);
    assert_eq!(result, "s3://my-bucket");
  }

  #[test]
  fn test_build_s3_store_uri_empty_config() {
    let cfg = S3CacheConfig::default();
    let result = build_s3_store_uri("s3://my-bucket", Some(&cfg));
    assert_eq!(result, "s3://my-bucket");
  }

  #[test]
  fn test_build_s3_store_uri_with_region() {
    let cfg = S3CacheConfig {
      region: Some("us-east-1".to_string()),
      ..Default::default()
    };
    let result = build_s3_store_uri("s3://my-bucket", Some(&cfg));
    assert_eq!(result, "s3://my-bucket?region=us-east-1");
  }

  #[test]
  fn test_build_s3_store_uri_with_endpoint_and_path_style() {
    let cfg = S3CacheConfig {
      endpoint_url: Some("https://minio.example.com".to_string()),
      use_path_style: true,
      ..Default::default()
    };
    let result = build_s3_store_uri("s3://my-bucket", Some(&cfg));
    assert!(result.starts_with("s3://my-bucket?"));
    assert!(result.contains("endpoint=https%3A%2F%2Fminio.example.com"));
    assert!(result.contains("use-path-style=true"));
  }

  #[test]
  fn test_build_s3_store_uri_all_params() {
    let cfg = S3CacheConfig {
      region: Some("eu-west-1".to_string()),
      endpoint_url: Some("https://s3.example.com".to_string()),
      use_path_style: true,
      ..Default::default()
    };
    let result = build_s3_store_uri("s3://cache-bucket", Some(&cfg));
    assert!(result.starts_with("s3://cache-bucket?"));
    assert!(result.contains("region=eu-west-1"));
    assert!(result.contains("endpoint=https%3A%2F%2Fs3.example.com"));
    assert!(result.contains("use-path-style=true"));
    // Verify params are joined with &
    assert_eq!(result.matches('&').count(), 2);
  }
}
