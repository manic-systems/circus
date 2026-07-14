use std::{path::PathBuf, sync::Arc, time::Duration};

use circus_common::{PgPool, alerts::AlertManager};
use circus_config::{
  BuilderSchedulingStrategy,
  CacheConfig,
  CacheUploadConfig,
  GcConfig,
  LogConfig,
  NotificationsConfig,
  SigningConfig,
};
use tokio::sync::Semaphore;

use crate::{caps::RunnerCaps, psi::PsiCache, rpc::AgentPool};

pub struct BuildContext {
  pub pool:                    PgPool,
  pub work_dir:                Arc<PathBuf>,
  pub nix_store_dir:           Arc<PathBuf>,
  pub timeout:                 Duration,
  pub max_silent_time:         Duration,
  pub log_config:              Arc<LogConfig>,
  pub gc_config:               Arc<GcConfig>,
  pub notifications_config:    NotificationsConfig,
  pub notification_secret_key: Option<String>,
  pub signing_config:          Arc<SigningConfig>,
  pub cache_config:            Arc<CacheConfig>,
  pub cache_upload_config:     Arc<CacheUploadConfig>,
  pub alert_manager:           Arc<Option<AlertManager>>,
  pub upload_semaphore:        Arc<Semaphore>,
  pub worker_semaphore:        Arc<Semaphore>,
  pub scheduling_strategy:     BuilderSchedulingStrategy,
  pub psi_threshold:           Option<f64>,
  pub psi_check_timeout:       Duration,
  pub psi_cache:               Arc<PsiCache>,
  pub extra_nix_args:          Arc<Vec<String>>,
  pub agent_pool:              Arc<AgentPool>,
  pub runner_caps:             Arc<RunnerCaps>,
  pub heartbeat_ttl:           Duration,
  pub require_host_key:        bool,
}
