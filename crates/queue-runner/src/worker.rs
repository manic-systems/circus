use std::{path::PathBuf, sync::Arc, time::Duration};

use circus_common::{
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
    EvaluationTriggerKind,
    metric_names,
    metric_units,
  },
  repo,
};
use dashmap::DashMap;
use sqlx::PgPool;
use tokio::sync::{RwLock, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::dispatch::supports_required_features;

pub type ActiveBuilds = Arc<DashMap<Uuid, CancellationToken>>;

pub struct WorkerPool {
  semaphore:           Arc<Semaphore>,
  upload_semaphore:    Arc<Semaphore>,
  worker_count:        usize,
  pool:                PgPool,
  #[expect(clippy::rc_buffer, reason = "shared config across tasks")]
  work_dir:            Arc<PathBuf>,
  #[expect(clippy::rc_buffer, reason = "shared config across tasks")]
  nix_store_dir:       Arc<PathBuf>,
  hot_config:          Arc<RwLock<HotConfig>>,
  log_config:          Arc<LogConfig>,
  gc_config:           Arc<GcConfig>,
  signing_config:      Arc<SigningConfig>,
  cache_upload_config: Arc<CacheUploadConfig>,
  alert_manager:       Arc<Option<AlertManager>>,
  psi_cache:           Arc<crate::psi::PsiCache>,
  agent_pool:          Arc<crate::rpc::AgentPool>,
  runner_caps:         Arc<crate::caps::RunnerCaps>,
  heartbeat_ttl:       Duration,
  drain_token:         CancellationToken,
  active_builds:       ActiveBuilds,
}

impl WorkerPool {
  #[expect(
    clippy::too_many_arguments,
    reason = "constructor wires long-lived runner dependencies once at startup"
  )]
  #[must_use]
  pub fn new(
    db_pool: PgPool,
    workers: usize,
    work_dir: PathBuf,
    nix_store_dir: PathBuf,
    hot_config: Arc<RwLock<HotConfig>>,
    log_config: LogConfig,
    gc_config: GcConfig,
    signing_config: SigningConfig,
    cache_upload_config: CacheUploadConfig,
    alert_config: Option<AlertConfig>,
    agent_pool: Arc<crate::rpc::AgentPool>,
    runner_caps: Arc<crate::caps::RunnerCaps>,
    heartbeat_ttl: Duration,
  ) -> Self {
    let alert_manager = alert_config.map(AlertManager::new);
    let upload_concurrency = cache_upload_config.upload_concurrency.max(1);
    Self {
      semaphore: Arc::new(Semaphore::new(workers)),
      upload_semaphore: Arc::new(Semaphore::new(upload_concurrency)),
      worker_count: workers,
      pool: db_pool,
      work_dir: Arc::new(work_dir),
      nix_store_dir: Arc::new(nix_store_dir),
      hot_config,
      log_config: Arc::new(log_config),
      gc_config: Arc::new(gc_config),
      signing_config: Arc::new(signing_config),
      cache_upload_config: Arc::new(cache_upload_config),
      alert_manager: Arc::new(alert_manager),
      psi_cache: crate::psi::PsiCache::new(),
      agent_pool,
      runner_caps,
      heartbeat_ttl,
      drain_token: CancellationToken::new(),
      active_builds: Arc::new(DashMap::new()),
    }
  }

  /// Signal all workers to stop accepting new builds. In-flight builds will
  /// finish.
  pub fn drain(&self) {
    self.drain_token.cancel();
  }

  /// Wait until all active builds finish. Agent builds are included even
  /// though they do not hold worker permits.
  pub async fn wait_for_drain(&self) {
    let build_timeout = self.hot_config.read().await.build_timeout;
    let _ = tokio::time::timeout(
      Duration::from_secs(build_timeout.as_secs() + 60),
      async {
        while !self.active_builds.is_empty() {
          tokio::time::sleep(Duration::from_millis(100)).await;
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
  pub const fn agent_pool(&self) -> &Arc<crate::rpc::AgentPool> {
    &self.agent_pool
  }

  #[must_use]
  pub const fn runner_caps(&self) -> &Arc<crate::caps::RunnerCaps> {
    &self.runner_caps
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

    let semaphore = Arc::clone(&self.semaphore);
    let upload_semaphore = Arc::clone(&self.upload_semaphore);
    let pool = self.pool.clone();
    let work_dir = Arc::clone(&self.work_dir);
    let nix_store_dir = Arc::clone(&self.nix_store_dir);
    let hot_config = Arc::clone(&self.hot_config);
    let log_config = Arc::clone(&self.log_config);
    let gc_config = Arc::clone(&self.gc_config);
    let signing_config = Arc::clone(&self.signing_config);
    let cache_upload_config = Arc::clone(&self.cache_upload_config);
    let alert_manager = Arc::clone(&self.alert_manager);
    let psi_cache = Arc::clone(&self.psi_cache);
    let agent_pool = Arc::clone(&self.agent_pool);
    let runner_caps = Arc::clone(&self.runner_caps);
    let heartbeat_ttl = self.heartbeat_ttl;
    let active_builds = Arc::clone(&self.active_builds);
    let cancel_token = CancellationToken::new();
    let build_id = build.id;

    active_builds.insert(build_id, cancel_token.clone());

    tokio::spawn(async move {
      let result = async {
        // Computed here so slow dry-runs on a cold queue don't serialize the
        // scheduler loop.
        let build =
          crate::features::ensure_effective_features(&pool, build).await;

        let (
          timeout,
          max_silent_time,
          notifications_config,
          scheduling_strategy,
          psi_threshold,
          psi_check_timeout,
          extra_nix_args,
          ssh_require_host_key,
        ) = {
          let hot = hot_config.read().await;
          (
            hot.build_timeout,
            hot.max_silent_time,
            hot.notifications_config.clone(),
            hot.scheduling_strategy.clone(),
            hot.psi_threshold,
            hot.psi_check_timeout,
            Arc::new(hot.extra_nix_build_args.clone()),
            hot.ssh_require_host_key,
          )
        };

        if let Err(e) = run_build(
          &pool,
          &build,
          &work_dir,
          &nix_store_dir,
          timeout,
          max_silent_time,
          &log_config,
          &gc_config,
          &notifications_config,
          &signing_config,
          &cache_upload_config,
          &alert_manager,
          Arc::clone(&upload_semaphore),
          Arc::clone(&semaphore),
          scheduling_strategy,
          psi_threshold,
          psi_check_timeout,
          Arc::clone(&psi_cache),
          extra_nix_args,
          Arc::clone(&agent_pool),
          Arc::clone(&runner_caps),
          heartbeat_ttl,
          ssh_require_host_key,
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

  let entry = first_path_info_entry(&parsed)?;
  let nar_hash = entry.get("narHash")?.as_str()?.to_string();
  let nar_size = entry.get("narSize")?.as_i64()?;

  Some((nar_hash, nar_size))
}

fn first_path_info_entry(
  parsed: &serde_json::Value,
) -> Option<&serde_json::Value> {
  if let Some(arr) = parsed.as_array() {
    arr.first()
  } else {
    parsed.as_object()?.values().next()
  }
}

/// Look up the project that owns a build (build -> evaluation -> jobset ->
/// project).
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

async fn is_interval_rebuild(pool: &PgPool, build: &Build) -> bool {
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

fn nix_args_for_build(
  base_args: &[String],
  interval_rebuild: bool,
) -> Vec<String> {
  let mut args = base_args.to_vec();
  if interval_rebuild && !args.iter().any(|arg| arg == "--rebuild") {
    args.push("--rebuild".to_string());
  }
  args
}

async fn dispatch_build_finished_notification(
  pool: &PgPool,
  build: &Build,
  notifications_config: &NotificationsConfig,
) {
  if let Some((project, commit_hash)) = get_project_for_build(pool, build).await
  {
    circus_common::notifications::dispatch_build_finished(
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
    Some(kf) => {
      tracing::info!(
        enabled = signing_config.enabled,
        key_file = %kf.display(),
        key_exists = kf.exists(),
        "Signing skipped: enabled=false or key_file missing"
      );
      return false;
    },
    None => {
      tracing::info!(
        enabled = signing_config.enabled,
        "Signing skipped: no key_file configured"
      );
      return false;
    },
  };

  let mut any_failed = false;
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
        tracing::info!(output = output_path, "Signed store path");
      },
      Ok(o) => {
        let stderr = String::from_utf8_lossy(&o.stderr);
        tracing::warn!(output = output_path, "Failed to sign: {stderr}");
        any_failed = true;
      },
      Err(e) => {
        tracing::warn!(
          output = output_path,
          "Failed to run nix store sign: {e}"
        );
        any_failed = true;
      },
    }
  }
  // "Signed" only if every path succeeded. A partial signing leaves cache
  // consumers unable to verify some outputs; surfacing that to the caller
  // lets it skip cache upload rather than push half-signed paths.
  !any_failed
}

/// Push output paths to an external binary cache via `nix copy`. Returns
/// the list of paths that exhausted their retry budget. An empty Vec
/// means every path made it.
async fn push_to_cache(
  output_paths: &[String],
  store_uri: &str,
  s3_config: Option<&circus_common::config::S3CacheConfig>,
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
  config: Option<&circus_common::config::S3CacheConfig>,
) -> String {
  let Some(cfg) = config else {
    return base_uri.to_string();
  };
  let base_uri =
    circus_common::s3::s3_store_uri_with_prefix(base_uri, Some(cfg));

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
    return base_uri;
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

fn presigned_s3_upload_available(config: &CacheUploadConfig) -> bool {
  if !config.enabled {
    return false;
  }
  let Some(store_uri) = config.store_uri.as_deref() else {
    return false;
  };
  let Some(s3_config) = config.s3.as_ref() else {
    return false;
  };
  circus_common::s3::Presigner::from_config(store_uri, s3_config).is_some()
}

/// Try to run the build on a remote builder if one is available for the build's
/// system.
#[expect(
  clippy::too_many_arguments,
  reason = "SSH fallback needs the same scheduling and execution context as \
            agent dispatch"
)]
async fn try_remote_build(
  pool: &PgPool,
  build: &Build,
  drv_path: &str,
  work_dir: &std::path::Path,
  timeout: Duration,
  live_log_path: Option<&std::path::Path>,
  strategy: &circus_common::config::BuilderSchedulingStrategy,
  psi_threshold: Option<f64>,
  psi_check_timeout: Duration,
  psi_cache: &crate::psi::PsiCache,
  extra_nix_args: &[String],
  require_host_key: bool,
) -> Option<crate::builder::BuildResult> {
  let system = build.system.as_deref()?;

  let builders = repo::remote_builders::find_for_system(pool, system, strategy)
    .await
    .ok()?;

  for builder in &builders {
    // Refuse unpinned builders when host-key verification is mandatory, rather
    // than silently falling back to trust-on-first-use.
    if require_host_key && builder.public_host_key.is_none() {
      tracing::warn!(
        build_id = %build.id,
        builder = %builder.name,
        "skipping builder: ssh_require_host_key is set but no public_host_key \
         is recorded"
      );
      continue;
    }
    // Leave the build pending for a builder with the right feature set.
    if !supports_required_features(
      build.scheduling_features(),
      &builder.supported_features,
      &builder.mandatory_features,
    ) {
      tracing::debug!(
        build_id = %build.id,
        builder = %builder.name,
        required = ?build.scheduling_features(),
        supported = ?builder.supported_features,
        mandatory = ?builder.mandatory_features,
        "skipping builder: missing required_features"
      );
      continue;
    }
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
    // Allow ssh-ng but default to ssh.
    let store_uri = if builder.ssh_uri.starts_with("ssh://")
      || builder.ssh_uri.starts_with("ssh-ng://")
    {
      builder.ssh_uri.clone()
    } else {
      format!("ssh://{}", builder.ssh_uri)
    };
    let result = crate::builder::run_nix_build_remote(
      drv_path,
      work_dir,
      timeout,
      &store_uri,
      builder.ssh_key_file.as_deref(),
      builder.public_host_key.as_deref(),
      live_log_path,
      extra_nix_args,
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

#[expect(clippy::ref_option, reason = "used as fn parameter pattern")]
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

/// Runs with a worker permit, trying configured SSH builders before local
/// execution. `Ok(None)` means no venue could take the build.
#[expect(
  clippy::too_many_arguments,
  reason = "on-runner execution needs the full SSH/local scheduling context"
)]
async fn run_on_runner(
  permit: tokio::sync::OwnedSemaphorePermit,
  pool: &PgPool,
  build: &Build,
  drv_path: &str,
  work_dir: &std::path::Path,
  timeout: Duration,
  live_log_path: &std::path::Path,
  scheduling_strategy: &circus_common::config::BuilderSchedulingStrategy,
  psi_threshold: Option<f64>,
  psi_check_timeout: Duration,
  psi_cache: &Arc<crate::psi::PsiCache>,
  extra_nix_args: &[String],
  runner_caps: &crate::caps::RunnerCaps,
  require_host_key: bool,
) -> circus_common::error::Result<Option<crate::builder::BuildResult>> {
  let _permit = permit;
  if build.system.is_some()
    && let Some(r) = try_remote_build(
      pool,
      build,
      drv_path,
      work_dir,
      timeout,
      Some(live_log_path),
      scheduling_strategy,
      psi_threshold,
      psi_check_timeout,
      psi_cache,
      extra_nix_args,
      require_host_key,
    )
    .await
  {
    return Ok(Some(r));
  }
  if !runner_caps.supports(build.system.as_deref(), build.scheduling_features())
  {
    tracing::warn!(
      build_id = %build.id,
      system = ?build.system,
      features = ?build.scheduling_features(),
      "no capable SSH builder and the runner host lacks the required \
       system/features; requeueing"
    );
    return Ok(None);
  }
  crate::builder::run_nix_build(
    drv_path,
    work_dir,
    timeout,
    Some(live_log_path),
    extra_nix_args,
  )
  .await
  .map(Some)
}

#[tracing::instrument(skip(pool, build, work_dir, nix_store_dir, log_config, gc_config, notifications_config, signing_config, cache_upload_config, upload_semaphore, scheduling_strategy), fields(build_id = %build.id, job = %build.job_name))]
#[expect(
  clippy::too_many_arguments,
  reason = "build execution coordinates database state, config, \
            notifications, cache upload, and scheduler handles"
)]
#[expect(clippy::ref_option, reason = "used as fn parameter pattern")]
#[expect(clippy::rc_buffer, reason = "extra args shared across calls")]
async fn run_build(
  pool: &PgPool,
  build: &Build,
  work_dir: &std::path::Path,
  nix_store_dir: &std::path::Path,
  timeout: Duration,
  max_silent_time: Duration,
  log_config: &LogConfig,
  gc_config: &GcConfig,
  notifications_config: &NotificationsConfig,
  signing_config: &SigningConfig,
  cache_upload_config: &CacheUploadConfig,
  alert_manager: &Option<AlertManager>,
  upload_semaphore: Arc<Semaphore>,
  worker_semaphore: Arc<Semaphore>,
  scheduling_strategy: circus_common::config::BuilderSchedulingStrategy,
  psi_threshold: Option<f64>,
  psi_check_timeout: Duration,
  psi_cache: Arc<crate::psi::PsiCache>,
  extra_nix_args: Arc<Vec<String>>,
  agent_pool: Arc<crate::rpc::AgentPool>,
  runner_caps: Arc<crate::caps::RunnerCaps>,
  heartbeat_ttl: Duration,
  require_host_key: bool,
) -> color_eyre::Result<()> {
  // Reserve capacity before claiming the build so `running` means execution
  // can start immediately.
  let Some(venue) = crate::dispatch::reserve_venue(
    &agent_pool,
    pool,
    build,
    build.system.as_deref(),
    psi_threshold,
    heartbeat_ttl,
    &scheduling_strategy,
    &worker_semaphore,
    &runner_caps,
    &psi_cache,
    psi_check_timeout,
    require_host_key,
  )
  .await
  else {
    return Ok(());
  };

  let Some(claimed_build) = repo::builds::start(pool, build.id).await? else {
    tracing::debug!(build_id = %build.id, "Build already claimed, skipping");
    return Ok(());
  };

  // Normalize drv_path: nix-eval-jobs always emits absolute store paths,
  // but manually-inserted or migrated rows may have bare filenames. Without
  // the leading slash nix resolves the path relative to work_dir and fails.
  let normalized_drv_path;
  let drv_path: &str = if build.drv_path.starts_with('/') {
    &build.drv_path
  } else {
    tracing::warn!(
      drv_path = %build.drv_path,
      "drv_path missing store prefix, normalizing"
    );
    normalized_drv_path =
      format!("{}/{}", nix_store_dir.display(), build.drv_path);
    &normalized_drv_path
  };

  let interval_rebuild = is_interval_rebuild(pool, build).await;
  let build_extra_nix_args =
    nix_args_for_build(&extra_nix_args, interval_rebuild);

  // Dispatch build started notification
  // If the project lookup fails, leave the at-most-once marker untouched.
  if let Some((project, commit_hash)) =
    get_project_for_build(pool, &claimed_build).await
  {
    match repo::builds::mark_started_notified(pool, build.id).await {
      Ok(true) => {
        circus_common::notifications::dispatch_build_started(
          pool,
          &claimed_build,
          &project,
          &commit_hash,
          notifications_config,
        )
        .await;
      },
      Ok(false) => {},
      Err(e) => {
        tracing::warn!(
          build_id = %build.id,
          "failed to mark started_notified: {e}"
        );
      },
    }
  }

  tracing::info!(build_id = %build.id, job = %build.job_name, "Starting build");

  // Clear stale steps from a prior requeued attempt.
  sqlx::query("DELETE FROM build_steps WHERE build_id = $1")
    .bind(build.id)
    .execute(pool)
    .await?;

  // Create a build step record
  let step = repo::build_steps::create(pool, CreateBuildStep {
    build_id:    build.id,
    step_number: 1,
    command:     if build_extra_nix_args.is_empty() {
      format!("nix build --no-link --print-out-paths {drv_path}")
    } else {
      format!(
        "nix build --no-link --print-out-paths {} {drv_path}",
        build_extra_nix_args.join(" ")
      )
    },
  })
  .await?;

  // Set up live log path
  let live_log_path =
    log_config.log_dir.join(format!("{}.active.log", build.id));
  let _ = tokio::fs::create_dir_all(&log_config.log_dir).await;

  let cache_upload_enabled_s3 =
    presigned_s3_upload_available(cache_upload_config);

  let result = match venue {
    crate::dispatch::ExecutionReservation::Agent { meta, snap, slot } => {
      let opts = crate::dispatch::AgentDispatch {
        timeout,
        max_silent_time,
        extra_nix_args: &build_extra_nix_args,
        cache_upload_enabled_s3,
        cache_upload_compression: &cache_upload_config.compression,
        fail_build_on_upload_error: cache_upload_config
          .fail_build_on_upload_error,
      };
      if let Some(r) = crate::dispatch::run_on_agent(
        &meta,
        &snap,
        slot,
        pool,
        build,
        drv_path,
        &live_log_path,
        &opts,
      )
      .await
      {
        Ok(Some(r))
      } else if let Ok(permit) =
        Arc::clone(&worker_semaphore).try_acquire_owned()
      {
        run_on_runner(
          permit,
          pool,
          build,
          drv_path,
          work_dir,
          timeout,
          &live_log_path,
          &scheduling_strategy,
          psi_threshold,
          psi_check_timeout,
          &psi_cache,
          &build_extra_nix_args,
          &runner_caps,
          require_host_key,
        )
        .await
      } else {
        Ok(None)
      }
    },
    crate::dispatch::ExecutionReservation::Runner(permit) => {
      run_on_runner(
        permit,
        pool,
        build,
        drv_path,
        work_dir,
        timeout,
        &live_log_path,
        &scheduling_strategy,
        psi_threshold,
        psi_check_timeout,
        &psi_cache,
        &build_extra_nix_args,
        &runner_caps,
        require_host_key,
      )
      .await
    },
  };

  // No venue executed the build, so hand it back to the queue. Only remove
  // the live log when requeuing succeeds, as cancelled or completed builds may
  // still need it.
  let result = match result {
    Ok(Some(r)) => Ok(r),
    Ok(None) => {
      match repo::builds::requeue(pool, build.id).await {
        Ok(Some(_)) => {
          let _ = tokio::fs::remove_file(&live_log_path).await;
        },
        Ok(None) => {
          tracing::debug!(
            build_id = %build.id,
            "build no longer running at requeue (cancelled or completed elsewhere)"
          );
        },
        Err(e) => {
          tracing::warn!(build_id = %build.id, "Failed to requeue after venue loss: {e}");
        },
      }
      return Ok(());
    },
    Err(e) => Err(e),
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
          if let Ok(gc_roots) = GcRoots::new(
            gc_config.gc_roots_dir.clone(),
            nix_store_dir.to_path_buf(),
            gc_config.enabled,
          ) {
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
        let upload_failed_paths = if cache_upload_config.enabled
          && !build_result.cache_upload_handled
          && let Some(ref store_uri) = cache_upload_config.store_uri
        {
          push_to_cache(
            &build_result.output_paths,
            store_uri,
            cache_upload_config.s3.as_ref(),
            Arc::clone(&upload_semaphore),
            cache_upload_config.upload_max_retries,
          )
          .await
        } else {
          Vec::new()
        };

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
          sqlx::query(
            "UPDATE builds SET status = 'pending', started_at = NULL, \
             retry_count = retry_count + 1, completed_at = NULL, \
             effective_features = NULL WHERE id = $1",
          )
          .bind(build.id)
          .execute(pool)
          .await?;
          if let Err(e) = tokio::fs::remove_file(&live_log_path).await {
            tracing::debug!(build_id = %build.id, "Failed to remove retry live log: {e}");
          }
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
      if let Err(e) = tokio::fs::remove_file(&live_log_path).await {
        tracing::debug!(build_id = %build.id, "Failed to remove failed live log: {e}");
      }

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
  use circus_common::config::{CacheUploadConfig, S3CacheConfig};

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
  fn test_build_s3_store_uri_with_prefix() {
    let cfg = S3CacheConfig {
      prefix: Some("nix-cache".to_string()),
      ..Default::default()
    };
    let result = build_s3_store_uri("s3://my-bucket/root", Some(&cfg));
    assert_eq!(result, "s3://my-bucket/root/nix-cache");
  }

  #[test]
  fn test_presigned_s3_upload_requires_explicit_credentials() {
    let missing_credentials = CacheUploadConfig {
      enabled: true,
      store_uri: Some("s3://my-bucket".to_string()),
      s3: Some(S3CacheConfig::default()),
      ..Default::default()
    };
    assert!(!presigned_s3_upload_available(&missing_credentials));

    let ready = CacheUploadConfig {
      enabled: true,
      store_uri: Some("s3://my-bucket".to_string()),
      s3: Some(S3CacheConfig {
        access_key_id: Some("AKIA".to_string()),
        secret_access_key: Some("secret".to_string()),
        ..Default::default()
      }),
      ..Default::default()
    };
    assert!(presigned_s3_upload_available(&ready));
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

  #[test]
  fn test_nix_args_for_interval_rebuild_adds_rebuild() {
    let args = nix_args_for_build(&["--print-build-logs".to_string()], true);
    assert_eq!(args, vec!["--print-build-logs", "--rebuild"]);
  }

  #[test]
  fn test_nix_args_for_interval_rebuild_does_not_duplicate() {
    let args = nix_args_for_build(&["--rebuild".to_string()], true);
    assert_eq!(args, vec!["--rebuild"]);
  }

  #[test]
  fn test_nix_args_for_source_build_keeps_base_args() {
    let args = nix_args_for_build(&["--print-build-logs".to_string()], false);
    assert_eq!(args, vec!["--print-build-logs"]);
  }
}
