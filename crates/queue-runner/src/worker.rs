use std::{
  collections::HashMap,
  path::{Path, PathBuf},
  sync::Arc,
  time::Duration,
};

use circus_common::{
  PgPool,
  alerts::AlertManager,
  gc_roots::GcRoots,
  log_storage::LogStorage,
  models::{
    Build,
    BuildStatus,
    CreateBuildProduct,
    CreateBuildStep,
    Project,
    metric_names,
    metric_units,
  },
  narinfo_signing::{read_signing_key, sign_narinfo},
  repo,
};
use circus_config::{
  AlertConfig,
  BuilderSchedulingStrategy,
  CacheConfig,
  CacheUploadConfig,
  GcConfig,
  HotConfig,
  LogConfig,
  NotificationsConfig,
  S3CacheConfig,
  SigningConfig,
};
use dashmap::DashMap;
use tokio::{
  fs,
  process::Command,
  sync::{OwnedSemaphorePermit, RwLock, Semaphore},
  time::{sleep, timeout},
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::{
  builder::{self as build_runner, BuildResult},
  caps::RunnerCaps,
  context::BuildContext,
  dispatch::{self, supports_required_features},
  features,
  helpers::{get_project_for_build, is_interval_rebuild},
  psi::{self, PsiCache},
  rpc::AgentPool,
};

#[derive(Debug, Clone)]
struct ClosurePathInfo {
  store_path: String,
  nar_hash:   String,
  nar_size:   i64,
  references: Vec<String>,
  deriver:    Option<String>,
  ca:         Option<String>,
}

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
  cache_config:        Arc<CacheConfig>,
  cache_upload_config: Arc<CacheUploadConfig>,
  alert_manager:       Arc<Option<AlertManager>>,
  psi_cache:           Arc<PsiCache>,
  agent_pool:          Arc<AgentPool>,
  runner_caps:         Arc<RunnerCaps>,
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
    cache_config: CacheConfig,
    cache_upload_config: CacheUploadConfig,
    alert_config: Option<AlertConfig>,
    agent_pool: Arc<AgentPool>,
    runner_caps: Arc<RunnerCaps>,
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
      cache_config: Arc::new(cache_config),
      cache_upload_config: Arc::new(cache_upload_config),
      alert_manager: Arc::new(alert_manager),
      psi_cache: PsiCache::new(),
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
    let _ = timeout(Duration::from_secs(build_timeout.as_secs() + 60), async {
      while !self.active_builds.is_empty() {
        sleep(Duration::from_millis(100)).await;
      }
    })
    .await;
  }

  #[must_use]
  pub const fn worker_count(&self) -> usize {
    self.worker_count
  }

  #[must_use]
  pub const fn agent_pool(&self) -> &Arc<AgentPool> {
    &self.agent_pool
  }

  #[must_use]
  pub const fn runner_caps(&self) -> &Arc<RunnerCaps> {
    &self.runner_caps
  }

  #[must_use]
  pub const fn active_builds(&self) -> &ActiveBuilds {
    &self.active_builds
  }

  pub async fn persist_closure_narinfos(
    &self,
    build_id: Uuid,
    output_paths: &[String],
    project_id: Option<Uuid>,
  ) {
    persist_closure_narinfos(
      &self.pool,
      build_id,
      output_paths,
      &self.signing_config,
      project_id,
    )
    .await;
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
    let cache_config = Arc::clone(&self.cache_config);
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
        let build = features::ensure_effective_features(&pool, build).await;

        let (
          timeout,
          max_silent_time,
          notifications_config,
          notification_secret_key,
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
            hot.notification_secret_key.clone(),
            hot.scheduling_strategy.clone(),
            hot.psi_threshold,
            hot.psi_check_timeout,
            Arc::new(hot.extra_nix_build_args.clone()),
            hot.ssh_require_host_key,
          )
        };

        let ctx = BuildContext {
          pool,
          work_dir,
          nix_store_dir,
          timeout,
          max_silent_time,
          log_config,
          gc_config,
          notifications_config,
          notification_secret_key,
          signing_config,
          cache_config,
          cache_upload_config,
          alert_manager,
          upload_semaphore,
          worker_semaphore: semaphore,
          scheduling_strategy,
          psi_threshold,
          psi_check_timeout,
          psi_cache,
          extra_nix_args,
          agent_pool,
          runner_caps,
          heartbeat_ttl,
          require_host_key: ssh_require_host_key,
        };

        if let Err(e) = run_build(ctx, &build).await {
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
  let output = Command::new("nix")
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
  let nar_hash = canonical_nix_sha256_hash(entry.get("narHash")?.as_str()?)?;
  let nar_size = entry.get("narSize")?.as_i64()?;

  Some((nar_hash, nar_size))
}

async fn get_recursive_path_infos_with_nix(
  nix: &Path,
  output_paths: &[String],
) -> Option<Vec<ClosurePathInfo>> {
  if output_paths.is_empty() {
    return Some(Vec::new());
  }

  let output = Command::new(nix)
    .args(["path-info", "--json", "--recursive"])
    .args(output_paths)
    .output()
    .await
    .ok()?;

  if !output.status.success() {
    tracing::warn!(
      stderr = %String::from_utf8_lossy(&output.stderr),
      "nix path-info --recursive failed while recording cache closure"
    );
    return None;
  }

  let parsed: serde_json::Value =
    serde_json::from_slice(&output.stdout).ok()?;
  Some(parse_recursive_path_infos(&parsed))
}

fn parse_recursive_path_infos(
  parsed: &serde_json::Value,
) -> Vec<ClosurePathInfo> {
  match parsed {
    serde_json::Value::Array(entries) => {
      entries
        .iter()
        .filter_map(|entry| parse_closure_path_info(None, entry))
        .collect()
    },
    serde_json::Value::Object(entries) => {
      entries
        .iter()
        .filter_map(|(path, entry)| parse_closure_path_info(Some(path), entry))
        .collect()
    },
    _ => Vec::new(),
  }
}

fn parse_closure_path_info(
  path_key: Option<&String>,
  entry: &serde_json::Value,
) -> Option<ClosurePathInfo> {
  let store_path = entry
    .get("path")
    .and_then(serde_json::Value::as_str)
    .or_else(|| path_key.map(String::as_str))?
    .to_string();
  let raw_nar_hash = entry.get("narHash")?.as_str()?;
  let Some(nar_hash) = canonical_nix_sha256_hash(raw_nar_hash) else {
    tracing::warn!(
      store_path = %store_path,
      nar_hash = %raw_nar_hash,
      "skipping closure path info with unsupported nar hash format"
    );
    return None;
  };
  let nar_size = entry.get("narSize")?.as_i64()?;
  let references = entry
    .get("references")
    .and_then(serde_json::Value::as_array)
    .map(|refs| {
      refs
        .iter()
        .filter_map(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .collect()
    })
    .unwrap_or_default();
  let deriver = entry
    .get("deriver")
    .and_then(serde_json::Value::as_str)
    .filter(|value| !value.is_empty())
    .map(ToOwned::to_owned);
  let ca = entry
    .get("ca")
    .and_then(serde_json::Value::as_str)
    .filter(|value| !value.is_empty())
    .map(ToOwned::to_owned);

  Some(ClosurePathInfo {
    store_path,
    nar_hash,
    nar_size,
    references,
    deriver,
    ca,
  })
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

fn store_hash_part(store_path: &str) -> Option<&str> {
  let name = store_path.rsplit('/').next()?;
  let (hash, _) = name.split_once('-')?;
  circus_nix::NixHash::is_valid(hash).then_some(hash)
}

fn nar_hash_key_segment(nar_hash: &str) -> Option<&str> {
  nar_hash
    .strip_prefix("sha256:")
    .filter(|hash| !hash.is_empty())
}

fn canonical_nix_sha256_hash(text: &str) -> Option<String> {
  if let Some(sri) = text.strip_prefix("sha256-") {
    let mut padded = sri.to_owned();
    while padded.len() % 4 != 0 {
      padded.push('=');
    }
    let bytes = {
      use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
      B64.decode(padded).ok()?
    };
    return canonical_sha256_bytes(&bytes);
  }

  let rest = text.strip_prefix("sha256:")?;
  if rest.len() == 52 && rest.bytes().all(circus_nix::base32::is_base32_byte) {
    return Some(text.to_owned());
  }
  if rest.len() == 64 && rest.bytes().all(|b| b.is_ascii_hexdigit()) {
    let bytes = hex::decode(rest).ok()?;
    return canonical_sha256_bytes(&bytes);
  }
  None
}

fn canonical_sha256_bytes(bytes: &[u8]) -> Option<String> {
  if bytes.len() != 32 {
    return None;
  }
  Some(format!(
    "sha256:{}",
    circus_nix::base32::encode_sha256(bytes)
  ))
}

fn nar_url_for_path(info: &ClosurePathInfo) -> Option<String> {
  let output_hash = store_hash_part(&info.store_path)?;
  let nar_hash = nar_hash_key_segment(&info.nar_hash)?;
  Some(format!("nar/{nar_hash}.nar?hash={output_hash}"))
}

async fn persist_closure_narinfos(
  pool: &PgPool,
  build_id: Uuid,
  output_paths: &[String],
  signing_config: &SigningConfig,
  project_id: Option<Uuid>,
) {
  persist_closure_narinfos_with_nix(
    Path::new("nix"),
    pool,
    build_id,
    output_paths,
    signing_config,
    project_id,
  )
  .await;
}

async fn persist_closure_narinfos_with_nix(
  nix: &Path,
  pool: &PgPool,
  build_id: Uuid,
  output_paths: &[String],
  signing_config: &SigningConfig,
  project_id: Option<Uuid>,
) {
  let key_file = match &signing_config.key_file {
    Some(kf) if signing_config.enabled && kf.exists() => kf,
    _ => return,
  };
  let signing_key = match read_signing_key(key_file).await {
    Ok(key) => key,
    Err(e) => {
      tracing::warn!(
        key_file = %key_file.display(),
        "failed to read closure narinfo signing key: {e}"
      );
      return;
    },
  };

  let Some(infos) = get_recursive_path_infos_with_nix(nix, output_paths).await
  else {
    return;
  };

  for info in infos {
    let Some(url) = nar_url_for_path(&info) else {
      tracing::warn!(
        store_path = %info.store_path,
        nar_hash = %info.nar_hash,
        "skipping closure narinfo with unsupported hash format"
      );
      continue;
    };

    let sig = sign_narinfo(
      &signing_key,
      &info.store_path,
      &info.nar_hash,
      info.nar_size,
      &info.references,
    );

    if let Err(e) =
      repo::narinfo_cache::upsert(pool, repo::narinfo_cache::UpsertNarInfo {
        store_path: &info.store_path,
        nar_hash: &info.nar_hash,
        nar_size: info.nar_size,
        file_hash: Some(&info.nar_hash),
        file_size: Some(info.nar_size),
        compression: "none",
        url: &url,
        deriver: info.deriver.as_deref(),
        references: &info.references,
        sig: Some(&sig),
        ca: info.ca.as_deref(),
        build_id: Some(build_id),
        project_id,
      })
      .await
    {
      tracing::warn!(
        build_id = %build_id,
        store_path = %info.store_path,
        "failed to persist closure narinfo: {e}"
      );
    }
  }
}

fn nix_args_for_build(
  base_args: &[String],
  interval_rebuild: bool,
  cache_args: Vec<String>,
) -> Vec<String> {
  let mut args = cache_args;
  args.extend_from_slice(base_args);
  if interval_rebuild && !args.iter().any(|arg| arg == "--rebuild") {
    args.push("--rebuild".to_string());
  }
  args
}

fn cache_args_for_build(
  config: &CacheConfig,
  project: Option<&Project>,
) -> Vec<String> {
  let (cache_url, upstreams): (Option<String>, Vec<(&str, Option<&str>)>) =
    if let Some(project) = project
      && project.cache_enabled
    {
      (
        circus_config::project_cache_url(
          config.cache_url.as_deref(),
          &project.name,
          project.cache_url.as_deref(),
        ),
        project
          .cache_upstreams
          .0
          .iter()
          .map(|upstream| {
            (upstream.url.as_str(), upstream.public_key.as_deref())
          })
          .collect(),
      )
    } else if config.enabled {
      (
        config.cache_url.clone(),
        config
          .upstreams
          .iter()
          .map(|upstream| {
            (upstream.url.as_str(), upstream.public_key.as_deref())
          })
          .collect(),
      )
    } else {
      (None, Vec::new())
    };

  let mut substituters = Vec::new();
  if let Some(cache_url) = cache_url.as_deref() {
    substituters.push(cache_url);
  }
  substituters.extend(upstreams.iter().map(|(url, _)| *url));

  let public_keys = upstreams
    .iter()
    .filter_map(|(_, key)| *key)
    .collect::<Vec<_>>();

  let mut args = Vec::new();
  if !substituters.is_empty() {
    args.push("--option".to_string());
    args.push("extra-substituters".to_string());
    args.push(substituters.join(" "));
  }
  if !public_keys.is_empty() {
    args.push("--option".to_string());
    args.push("extra-trusted-public-keys".to_string());
    args.push(public_keys.join(" "));
  }
  args
}

async fn dispatch_build_finished_notification(
  pool: &PgPool,
  build: &Build,
  notifications_config: &NotificationsConfig,
  notification_secret_key: Option<&str>,
) {
  if let Some((project, commit_hash)) = get_project_for_build(pool, build).await
  {
    circus_notification::dispatch_build_finished(
      Some(pool),
      build,
      &project,
      &commit_hash,
      notifications_config,
      notification_secret_key,
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
    let result = Command::new("nix")
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
  s3_config: Option<&S3CacheConfig>,
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
      let result = Command::new("nix")
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
            sleep(Duration::from_secs(2u64.pow(attempt))).await;
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
            sleep(Duration::from_secs(2u64.pow(attempt))).await;
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
  config: Option<&S3CacheConfig>,
) -> String {
  let Some(cfg) = config else {
    return base_uri.to_string();
  };
  let base_uri = circus_s3::s3_store_uri_with_prefix(base_uri, Some(cfg));

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
  circus_s3::Presigner::from_config(store_uri, s3_config).is_some()
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
  work_dir: &Path,
  timeout: Duration,
  live_log_path: Option<&Path>,
  strategy: &BuilderSchedulingStrategy,
  psi_threshold: Option<f64>,
  psi_check_timeout: Duration,
  psi_cache: &PsiCache,
  extra_nix_args: &[String],
  require_host_key: bool,
) -> Option<BuildResult> {
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
        psi::read_cached(psi_cache, &builder.ssh_uri, psi_check_timeout).await
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
    let result = build_runner::run_nix_build_remote(
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
    if let Ok(meta) = fs::metadata(path).await {
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
  permit: OwnedSemaphorePermit,
  pool: &PgPool,
  build: &Build,
  drv_path: &str,
  work_dir: &Path,
  timeout: Duration,
  live_log_path: &Path,
  scheduling_strategy: &BuilderSchedulingStrategy,
  psi_threshold: Option<f64>,
  psi_check_timeout: Duration,
  psi_cache: &Arc<PsiCache>,
  extra_nix_args: &[String],
  runner_caps: &RunnerCaps,
  require_host_key: bool,
) -> circus_common::error::Result<Option<BuildResult>> {
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
  build_runner::run_nix_build(
    drv_path,
    work_dir,
    timeout,
    Some(live_log_path),
    extra_nix_args,
  )
  .await
  .map(Some)
}

#[tracing::instrument(skip(ctx, build), fields(build_id = %build.id, job = %build.job_name))]
async fn run_build(ctx: BuildContext, build: &Build) -> color_eyre::Result<()> {
  // Reserve capacity before claiming the build so `running` means execution
  // can start immediately.
  let Some(venue) =
    dispatch::reserve_venue(&ctx, build, build.system.as_deref()).await
  else {
    return Ok(());
  };

  let BuildContext {
    pool,
    work_dir,
    nix_store_dir,
    timeout,
    max_silent_time,
    log_config,
    gc_config,
    notifications_config,
    notification_secret_key,
    signing_config,
    cache_config,
    cache_upload_config,
    alert_manager,
    upload_semaphore,
    worker_semaphore,
    scheduling_strategy,
    psi_threshold,
    psi_check_timeout,
    psi_cache,
    extra_nix_args,
    runner_caps,
    require_host_key,
    ..
  } = ctx;
  let pool = &pool;
  let work_dir = work_dir.as_path();
  let nix_store_dir = nix_store_dir.as_path();
  let log_config = log_config.as_ref();
  let gc_config = gc_config.as_ref();
  let notifications_config = &notifications_config;
  let signing_config = signing_config.as_ref();
  let cache_config = cache_config.as_ref();
  let cache_upload_config = cache_upload_config.as_ref();
  let alert_manager = alert_manager.as_ref();

  let Some(claimed_build) = repo::builds::start(pool, build.id).await? else {
    tracing::debug!(build_id = %build.id, "Build already claimed, skipping");
    return Ok(());
  };

  // Normalize drv_path to an absolute store path: rows inserted manually or
  // via migration may have bare filenames. Without the leading slash nix
  // resolves the path relative to work_dir and fails.
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

  let project_context = get_project_for_build(pool, &claimed_build).await;
  let interval_rebuild = is_interval_rebuild(pool, build).await;
  let build_extra_nix_args = nix_args_for_build(
    &extra_nix_args,
    interval_rebuild,
    cache_args_for_build(
      cache_config,
      project_context.as_ref().map(|(p, _)| p),
    ),
  );

  // Dispatch build started notification
  // If the project lookup fails, leave the at-most-once marker untouched.
  if let Some((project, commit_hash)) = project_context.as_ref() {
    match repo::builds::mark_started_notified(pool, build.id).await {
      Ok(true) => {
        circus_notification::dispatch_build_started(
          pool,
          &claimed_build,
          project,
          commit_hash,
          notifications_config,
          notification_secret_key.as_deref(),
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
  repo::build_steps::delete_for_build(pool, build.id).await?;

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
  let _ = fs::create_dir_all(&log_config.log_dir).await;

  let cache_upload_enabled_s3 =
    presigned_s3_upload_available(cache_upload_config);

  let result = match venue {
    dispatch::ExecutionReservation::Agent { meta, snap, slot } => {
      let opts = dispatch::AgentDispatch {
        timeout,
        max_silent_time,
        extra_nix_args: &build_extra_nix_args,
        cache_upload_enabled_s3,
        cache_upload_compression: &cache_upload_config.compression,
        fail_build_on_upload_error: cache_upload_config
          .fail_build_on_upload_error,
      };
      if let Some(r) = dispatch::run_on_agent(
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
    dispatch::ExecutionReservation::Runner(permit) => {
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
          let _ = fs::remove_file(&live_log_path).await;
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
          if let Err(e) = fs::rename(&live_log_path, &final_path).await {
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
        let path_to_name: HashMap<String, String> = build
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
            repo::build_products::set_gc_root_path(
              pool,
              product.id,
              gc_root_path.as_deref(),
            )
            .await?;
          }
        }

        // Sign outputs at build time
        if sign_outputs(&build_result.output_paths, signing_config).await
          && let Err(e) = repo::builds::mark_signed(pool, build.id).await
        {
          tracing::warn!(build_id = %build.id, "Failed to mark build as signed: {e}");
        }

        persist_closure_narinfos(
          pool,
          build.id,
          &build_result.output_paths,
          signing_config,
          project_context.as_ref().map(|(project, _)| project.id),
        )
        .await;

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
            notification_secret_key.as_deref(),
          )
          .await;
          return Ok(());
        }

        let primary_output =
          build_result.output_paths.first().map(String::as_str);

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
          repo::builds::retry(pool, build.id).await?;
          if let Err(e) = fs::remove_file(&live_log_path).await {
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
      if let Err(e) = fs::remove_file(&live_log_path).await {
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
      notification_secret_key.as_deref(),
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
  use circus_common::models::{BinaryCacheUpstream, BinaryCacheUpstreams};
  use circus_config::{CacheUploadConfig, S3CacheConfig};

  use super::*;

  #[test]
  fn test_canonical_nix_sha256_hash_accepts_common_formats() {
    let bytes = [7u8; 32];
    let nix32 = circus_nix::base32::encode_sha256(&bytes);
    let expected = format!("sha256:{nix32}");

    assert_eq!(
      canonical_nix_sha256_hash(&format!("sha256:{nix32}")).as_deref(),
      Some(expected.as_str())
    );
    assert_eq!(
      canonical_nix_sha256_hash(&format!("sha256:{}", hex::encode(bytes)))
        .as_deref(),
      Some(expected.as_str())
    );
    assert_eq!(
      canonical_nix_sha256_hash(&format!("sha256-{}", {
        use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
        B64.encode(bytes)
      }))
      .as_deref(),
      Some(expected.as_str())
    );
  }

  #[test]
  fn test_parse_recursive_path_infos_canonicalizes_sri_nar_hashes() {
    let bytes = [11u8; 32];
    let sri_hash = {
      use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
      format!("sha256-{}", B64.encode(bytes))
    };
    let expected_hash =
      format!("sha256:{}", circus_nix::base32::encode_sha256(&bytes));
    let parsed = serde_json::json!({
      "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-linux-6.18.33-valve2": {
        "narHash": sri_hash,
        "narSize": 1234,
        "references": [
          "/nix/store/bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb-glibc"
        ],
        "deriver": "/nix/store/cccccccccccccccccccccccccccccccc-linux.drv",
        "ca": null
      },
      "/nix/store/dddddddddddddddddddddddddddddddd-bad-hash": {
        "narHash": "md5:0123456789abcdef0123456789abcdef",
        "narSize": 99,
        "references": []
      }
    });

    let infos = parse_recursive_path_infos(&parsed);

    assert_eq!(infos.len(), 1);
    assert_eq!(
      infos[0].store_path,
      "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-linux-6.18.33-valve2"
    );
    assert_eq!(infos[0].nar_hash, expected_hash);
  }

  #[tokio::test]
  async fn test_persist_closure_narinfos_records_dependency_closure() {
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
      return;
    };

    circus_migrations::run_migrations(&url)
      .await
      .expect("migration failed");
    let pool = circus_common::build_pool(&url, 5).expect("failed to connect");

    let project = repo::projects::create(&pool, circus_common::CreateProject {
      name:            format!("closure-cache-{}", Uuid::new_v4().simple()),
      description:     None,
      repository_url:  "https://github.com/test/closure-cache".to_string(),
      cache_enabled:   true,
      cache_url:       None,
      cache_upstreams: BinaryCacheUpstreams::default(),
    })
    .await
    .expect("create project");
    let jobset = repo::jobsets::create(&pool, circus_common::CreateJobset {
      project_id:        project.id,
      name:              "main".to_string(),
      nix_expression:    "packages".to_string(),
      enabled:           None,
      flake_mode:        None,
      check_interval:    None,
      trigger_mode:      None,
      branch:            None,
      branch_pattern:    None,
      tag_pattern:       None,
      scheduling_shares: None,
      state:             None,
      keep_nr:           None,
    })
    .await
    .expect("create jobset");
    let evaluation =
      repo::evaluations::create(&pool, circus_common::CreateEvaluation {
        jobset_id:      jobset.id,
        commit_hash:    Uuid::new_v4().simple().to_string(),
        pr_number:      None,
        pr_head_branch: None,
        pr_base_branch: None,
        pr_action:      None,
      })
      .await
      .expect("create evaluation");
    let build = repo::builds::create(&pool, circus_common::CreateBuild {
      evaluation_id: evaluation.id,
      job_name: "closure-cache-job".to_string(),
      drv_path: format!(
        "/nix/store/{}-closure-cache-job.drv",
        "33333333333333333333333333333333"
      ),
      system: Some("x86_64-linux".to_string()),
      outputs: None,
      is_aggregate: None,
      constituents: None,
      is_fod: None,
      fod_hash: None,
      ..Default::default()
    })
    .await
    .expect("create build");

    let temp_dir = std::env::temp_dir()
      .join(format!("circus-closure-cache-{}", Uuid::new_v4().simple()));
    tokio::fs::create_dir_all(&temp_dir)
      .await
      .expect("create temp dir");
    let key_file = temp_dir.join("signing.key");
    let fake_nix = temp_dir.join("nix");

    let signing_key = "circus-test-1:\
                       OlzHrxDxaOpPjkL5uNXF77Xq4VRiz6Zy0LqlK6GCNqRX90gxFy2HSr/\
                       hxqdpc2VMU2UIlDOAEBv842MCsbPfgQ==";
    tokio::fs::write(&key_file, signing_key)
      .await
      .expect("write signing key");

    let output_store_hash = "11111111111111111111111111111111";
    let dep_store_hash = "22222222222222222222222222222222";
    let output_path = format!("/nix/store/{output_store_hash}-output");
    let dep_path = format!("/nix/store/{dep_store_hash}-dependency");
    let output_nar_bytes = [21u8; 32];
    let dep_nar_bytes = [22u8; 32];
    let output_sri = {
      use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
      format!("sha256-{}", B64.encode(output_nar_bytes))
    };
    let dep_sri = {
      use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
      format!("sha256-{}", B64.encode(dep_nar_bytes))
    };
    let output_nar_hash = format!(
      "sha256:{}",
      circus_nix::base32::encode_sha256(&output_nar_bytes)
    );
    let dep_nar_hash = format!(
      "sha256:{}",
      circus_nix::base32::encode_sha256(&dep_nar_bytes)
    );
    let path_info = serde_json::json!({
      output_path.clone(): {
        "narHash": output_sri,
        "narSize": 123,
        "references": [dep_path.clone()],
        "deriver": "/nix/store/44444444444444444444444444444444-output.drv",
        "ca": null
      },
      dep_path.clone(): {
        "narHash": dep_sri,
        "narSize": 45,
        "references": [],
        "deriver": serde_json::Value::Null,
        "ca": null
      }
    });
    tokio::fs::write(
      &fake_nix,
      format!("#!/bin/sh\ncat <<'JSON'\n{path_info}\nJSON\n"),
    )
    .await
    .expect("write fake nix");
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;

      let mut permissions = std::fs::metadata(&fake_nix)
        .expect("stat fake nix")
        .permissions();
      permissions.set_mode(0o755);
      std::fs::set_permissions(&fake_nix, permissions).expect("chmod fake nix");
    }

    persist_closure_narinfos_with_nix(
      &fake_nix,
      &pool,
      build.id,
      std::slice::from_ref(&output_path),
      &SigningConfig {
        enabled:  true,
        key_file: Some(key_file),
      },
      Some(project.id),
    )
    .await;

    let output_row = repo::narinfo_cache::get(&pool, &output_path)
      .await
      .expect("output narinfo row");
    let dep_row = repo::narinfo_cache::get(&pool, &dep_path)
      .await
      .expect("dependency narinfo row");

    assert_eq!(output_row.build_id, Some(build.id));
    assert_eq!(output_row.project_id, Some(project.id));
    assert_eq!(output_row.nar_hash, output_nar_hash);
    assert_eq!(
      output_row.url,
      format!(
        "nar/{}.nar?hash={output_store_hash}",
        output_nar_hash
          .strip_prefix("sha256:")
          .expect("test nar hash should use sha256 prefix")
      )
    );
    assert_eq!(output_row.references, vec![dep_path.clone()]);
    assert!(output_row.sig.as_deref().is_some_and(|sig| {
      sig.starts_with("circus-test-1:") && sig.len() > "circus-test-1:".len()
    }));

    assert_eq!(dep_row.build_id, Some(build.id));
    assert_eq!(dep_row.project_id, Some(project.id));
    assert_eq!(dep_row.nar_hash, dep_nar_hash);
    assert!(dep_row.references.is_empty());
    assert!(dep_row.sig.as_deref().is_some_and(|sig| {
      sig.starts_with("circus-test-1:") && sig.len() > "circus-test-1:".len()
    }));

    let _ = repo::projects::delete(&pool, project.id).await;
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
  }

  #[test]
  fn test_cache_args_use_project_cache_and_upstreams() {
    let cache_config = CacheConfig {
      enabled: true,
      cache_url: Some("https://ci.example.org/nix-cache/".to_string()),
      upstreams: vec![circus_config::BinaryCacheUpstream {
        url:        "https://global-cache.example.org/".to_string(),
        public_key: Some("global-1:key".to_string()),
      }],
      ..Default::default()
    };
    let project = Project {
      id:              Uuid::new_v4(),
      name:            "project-a".to_string(),
      description:     None,
      repository_url:  "https://example.org/project-a.git".to_string(),
      cache_enabled:   true,
      cache_url:       Some(
        "https://ci.example.org/projects/project-a/nix-cache/".to_string(),
      ),
      cache_upstreams: BinaryCacheUpstreams(vec![BinaryCacheUpstream {
        url:        "https://cache.nixos.org/".to_string(),
        public_key: Some("cache.nixos.org-1:key".to_string()),
      }]),
      created_at:      chrono::Utc::now(),
      updated_at:      chrono::Utc::now(),
    };

    let args = cache_args_for_build(&cache_config, Some(&project));

    assert_eq!(
      args,
      vec![
        "--option",
        "extra-substituters",
        "https://ci.example.org/projects/project-a/nix-cache/ https://cache.nixos.org/",
        "--option",
        "extra-trusted-public-keys",
        "cache.nixos.org-1:key",
      ]
    );
  }

  #[test]
  fn test_cache_args_derive_project_cache_when_global_cache_disabled() {
    let cache_config = CacheConfig {
      enabled: false,
      cache_url: Some("https://ci.example.org/nix-cache/".to_string()),
      ..Default::default()
    };
    let project = Project {
      id:              Uuid::new_v4(),
      name:            "project-a".to_string(),
      description:     None,
      repository_url:  "https://example.org/project-a.git".to_string(),
      cache_enabled:   true,
      cache_url:       None,
      cache_upstreams: BinaryCacheUpstreams::default(),
      created_at:      chrono::Utc::now(),
      updated_at:      chrono::Utc::now(),
    };

    let args = cache_args_for_build(&cache_config, Some(&project));

    assert_eq!(args, vec![
      "--option",
      "extra-substituters",
      "https://ci.example.org/projects/project-a/nix-cache/",
    ]);
  }

  #[test]
  fn test_cache_args_fall_back_to_global_cache_when_project_cache_disabled() {
    let cache_config = CacheConfig {
      enabled: true,
      cache_url: Some("https://ci.example.org/nix-cache/".to_string()),
      upstreams: vec![circus_config::BinaryCacheUpstream {
        url:        "https://global-cache.example.org/".to_string(),
        public_key: Some("global-1:key".to_string()),
      }],
      ..Default::default()
    };
    let project = Project {
      id:              Uuid::new_v4(),
      name:            "project-a".to_string(),
      description:     None,
      repository_url:  "https://example.org/project-a.git".to_string(),
      cache_enabled:   false,
      cache_url:       Some(
        "https://ci.example.org/projects/project-a/nix-cache/".to_string(),
      ),
      cache_upstreams: BinaryCacheUpstreams(vec![BinaryCacheUpstream {
        url:        "https://cache.nixos.org/".to_string(),
        public_key: Some("cache.nixos.org-1:key".to_string()),
      }]),
      created_at:      chrono::Utc::now(),
      updated_at:      chrono::Utc::now(),
    };

    let args = cache_args_for_build(&cache_config, Some(&project));

    assert_eq!(args, vec![
      "--option",
      "extra-substituters",
      "https://ci.example.org/nix-cache/ https://global-cache.example.org/",
      "--option",
      "extra-trusted-public-keys",
      "global-1:key",
    ]);
  }

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
    let args =
      nix_args_for_build(&["--print-build-logs".to_string()], true, Vec::new());
    assert_eq!(args, vec!["--print-build-logs", "--rebuild"]);
  }

  #[test]
  fn test_nix_args_for_interval_rebuild_does_not_duplicate() {
    let args = nix_args_for_build(&["--rebuild".to_string()], true, Vec::new());
    assert_eq!(args, vec!["--rebuild"]);
  }

  #[test]
  fn test_nix_args_for_source_build_keeps_base_args() {
    let args = nix_args_for_build(
      &["--print-build-logs".to_string()],
      false,
      Vec::new(),
    );
    assert_eq!(args, vec!["--print-build-logs"]);
  }
}
