//! The running state should mean a build already holds execution capacity.

use std::{
  path::Path,
  sync::Arc,
  time::{Duration, Instant},
};

use circus_common::{config::BuilderSchedulingStrategy, models::Build, repo};
use sqlx::PgPool;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};

use crate::{
  builder::BuildResult,
  rpc::{
    AgentPool,
    AgentSnapshot,
    pool::{
      AgentMeta,
      DispatchCommand,
      DispatchResult,
      PresignedUpload,
      SlotGuard,
    },
  },
};

pub enum ExecutionReservation {
  Agent {
    meta: Arc<AgentMeta>,
    snap: AgentSnapshot,
    slot: SlotGuard,
  },
  Runner(OwnedSemaphorePermit),
}

/// `list_pending` gets one capacity value for the whole fleet. This is only a
/// fairness estimate, while reservations decide what can actually run.
pub struct SchedulerCapacity {
  pub fetch_limit:          i64,
  pub schedulable_capacity: i32,
}

#[must_use]
pub fn scheduler_capacity(
  agent_pool: &AgentPool,
  worker_count: usize,
) -> SchedulerCapacity {
  let workers = worker_count as i64;
  SchedulerCapacity {
    fetch_limit:          workers
      .saturating_add(i64::from(agent_pool.total_free_slots()))
      .clamp(10, 512),
    schedulable_capacity: workers
      .saturating_add(i64::from(agent_pool.total_slots()))
      .clamp(1, i64::from(i32::MAX)) as i32,
  }
}

#[must_use]
pub(crate) fn supports_required_features(
  required_features: &[String],
  supported_features: &[String],
  mandatory_features: &[String],
) -> bool {
  required_features
    .iter()
    .all(|feature| supported_features.contains(feature))
    && mandatory_features
      .iter()
      .all(|feature| required_features.contains(feature))
}
pub struct AgentDispatch<'a> {
  pub timeout:                    Duration,
  pub max_silent_time:            Duration,
  pub extra_nix_args:             &'a [String],
  pub cache_upload_enabled_s3:    bool,
  pub cache_upload_compression:   &'a str,
  pub fail_build_on_upload_error: bool,
}

/// Reserve capacity before the build is claimed as running.
///
/// # Panics
///
/// Only if the worker semaphore has been closed, which never happens during
/// normal operation.
pub async fn reserve_venue(
  agent_pool: &Arc<AgentPool>,
  pool: &PgPool,
  build: &Build,
  system: Option<&str>,
  psi_threshold: Option<f64>,
  heartbeat_ttl: Duration,
  strategy: &BuilderSchedulingStrategy,
  worker_semaphore: &Arc<Semaphore>,
) -> ExecutionReservation {
  if let Some(system) = system
    && let Some((meta, snap, slot)) = select_and_reserve_agent(
      agent_pool,
      pool,
      build,
      system,
      psi_threshold,
      heartbeat_ttl,
      strategy,
    )
    .await
  {
    return ExecutionReservation::Agent { meta, snap, slot };
  }

  #[expect(
    clippy::expect_used,
    reason = "the worker semaphore is never closed, so acquire never errors"
  )]
  let permit = Arc::clone(worker_semaphore)
    .acquire_owned()
    .await
    .expect("worker semaphore is never closed");
  ExecutionReservation::Runner(permit)
}

async fn select_and_reserve_agent(
  agent_pool: &Arc<AgentPool>,
  pool: &PgPool,
  build: &Build,
  system: &str,
  psi_threshold: Option<f64>,
  heartbeat_ttl: Duration,
  strategy: &BuilderSchedulingStrategy,
) -> Option<(Arc<AgentMeta>, AgentSnapshot, SlotGuard)> {
  use BuilderSchedulingStrategy::{
    CpuCoreCountWithSpeedFactor,
    Dynamic,
    SpeedFactorOnly,
  };

  let mut candidates = agent_pool.candidates_for(system);
  if candidates.is_empty() {
    return None;
  }

  // Missing or stale heartbeats are treated as unknown to match the SSH path.
  let cutoff = Instant::now().checked_sub(heartbeat_ttl);
  if let Some(t) = psi_threshold {
    let t = t as f32;
    candidates.retain(|(_, snap)| {
      let hb = snap.heartbeat;
      let fresh = match (hb.last_seen, cutoff) {
        (Some(seen), Some(cut)) => seen >= cut,
        _ => true,
      };
      if !fresh {
        return true;
      }
      hb.cpu_psi_avg10 <= t && hb.mem_psi_avg10 <= t && hb.io_psi_avg10 <= t
    });
  }

  candidates.retain(|(_, snap)| {
    supports_required_features(
      &build.required_features,
      &snap.supported_features,
      &snap.mandatory_features,
    )
  });
  if candidates.is_empty() {
    return None;
  }

  let mut eligible = Vec::with_capacity(candidates.len());
  for candidate in candidates {
    match repo::builder_sessions::is_schedulable(pool, candidate.0.machine_id)
      .await
    {
      Ok(true) => eligible.push(candidate),
      Ok(false) => {
        tracing::debug!(
          machine_id = %candidate.0.machine_id,
          name = %candidate.1.name,
          "skipping agent disabled by failure backoff"
        );
      },
      Err(e) => {
        tracing::warn!(
          machine_id = %candidate.0.machine_id,
          name = %candidate.1.name,
          "failed to read agent backoff state: {e}"
        );
      },
    }
  }

  eligible.sort_by(|a, b| {
    match strategy {
      SpeedFactorOnly => {
        b.1
          .speed_factor
          .partial_cmp(&a.1.speed_factor)
          .unwrap_or(std::cmp::Ordering::Equal)
      },
      CpuCoreCountWithSpeedFactor => {
        let av = a.1.cpu_count as f32 * a.1.speed_factor;
        let bv = b.1.cpu_count as f32 * b.1.speed_factor;
        bv.partial_cmp(&av).unwrap_or(std::cmp::Ordering::Equal)
      },
      Dynamic => {
        let free = |s: &AgentSnapshot| -> f32 {
          s.max_jobs.saturating_sub(s.current_jobs) as f32 * s.speed_factor
        };
        free(&b.1)
          .partial_cmp(&free(&a.1))
          .unwrap_or(std::cmp::Ordering::Equal)
      },
    }
  });

  eligible.into_iter().find_map(|(meta, snap)| {
    meta.try_acquire_slot().map(|slot| (meta, snap, slot))
  })
}

/// Return [`None`] when the agent disappears before reporting a result.
pub async fn run_on_agent(
  meta: &Arc<AgentMeta>,
  snap: &AgentSnapshot,
  slot: SlotGuard,
  pool: &PgPool,
  build: &Build,
  drv_path: &str,
  live_log_path: &Path,
  opts: &AgentDispatch<'_>,
) -> Option<BuildResult> {
  let (tx, rx) = oneshot::channel();
  let presigned_upload = opts.cache_upload_enabled_s3.then(|| {
    PresignedUpload {
      compression:                opts.cache_upload_compression.to_owned(),
      fail_build_on_upload_error: opts.fail_build_on_upload_error,
    }
  });

  let cmd = DispatchCommand {
    build_id: build.id,
    drv_path: drv_path.to_owned(),
    max_log_size: 100 * 1024 * 1024,
    max_silent_time: opts
      .max_silent_time
      .as_secs()
      .try_into()
      .unwrap_or(u32::MAX),
    build_timeout: opts.timeout.as_secs().try_into().unwrap_or(u32::MAX),
    extra_args: opts.extra_nix_args.to_vec(),
    log_path: live_log_path.to_path_buf(),
    presigned_upload,
    reservation: slot,
    completion: tx,
  };
  if meta.tx.send(cmd).is_err() {
    tracing::warn!(name = %snap.name, "agent channel closed, falling back");
    return None;
  }

  if let Err(e) = sqlx::query(
    "UPDATE builder_sessions SET updated_at = NOW() WHERE machine_id = $1",
  )
  .bind(meta.machine_id)
  .execute(pool)
  .await
  {
    tracing::debug!(name = %snap.name, "builder_sessions touch failed: {e}");
  }
  if let Err(e) = repo::builds::set_agent(pool, build.id, meta.machine_id).await
  {
    tracing::warn!(build_id = %build.id, name = %snap.name, "Failed to set agent_machine_id: {e}");
  }
  tracing::info!(build_id = %build.id, agent = %snap.name, "dispatched to agent");

  let result = |success, exit_code, stderr: String, output_paths| {
    BuildResult {
      success,
      exit_code: Some(exit_code),
      stdout: String::new(),
      stderr,
      output_paths,
      sub_steps: Vec::new(),
      cache_upload_handled: opts.cache_upload_enabled_s3,
    }
  };

  match rx.await {
    Ok(DispatchResult::Succeeded) => {
      let outputs = read_drv_outputs(drv_path).await;
      Some(result(true, 0, String::new(), outputs))
    },
    Ok(DispatchResult::Failed(error_message)) => {
      Some(result(false, 1, error_message, Vec::new()))
    },
    Ok(DispatchResult::TimedOut) => {
      Some(result(false, 124, "build timed out".into(), Vec::new()))
    },
    Ok(DispatchResult::Aborted) => {
      Some(result(false, 130, "build aborted".into(), Vec::new()))
    },
    Ok(DispatchResult::Disconnected) | Err(_) => {
      tracing::warn!(name = %snap.name, "agent disconnected mid-build; falling back");
      None
    },
  }
}

async fn read_drv_outputs(drv_path: &str) -> Vec<String> {
  let Ok(out) = tokio::process::Command::new("nix-store")
    .args(["--query", "--outputs", drv_path])
    .output()
    .await
  else {
    return Vec::new();
  };
  if !out.status.success() {
    return Vec::new();
  }
  String::from_utf8_lossy(&out.stdout)
    .lines()
    .map(|s| s.trim().to_owned())
    .filter(|s| !s.is_empty())
    .collect()
}

#[cfg(test)]
mod tests {
  use super::supports_required_features;

  fn strs(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
  }

  #[test]
  fn supported_features_must_cover_build_requirements() {
    assert!(supports_required_features(
      &strs(&["kvm", "nixos-test"]),
      &strs(&["benchmark", "kvm", "nixos-test"]),
      &[],
    ));
    assert!(!supports_required_features(
      &strs(&["kvm", "nixos-test", "uid-range"]),
      &strs(&["benchmark", "kvm", "nixos-test"]),
      &[],
    ));
  }

  #[test]
  fn builder_mandatory_features_must_be_required_by_build() {
    assert!(supports_required_features(
      &strs(&["kvm", "nixos-test"]),
      &strs(&["kvm", "nixos-test"]),
      &strs(&["kvm"]),
    ));
    assert!(!supports_required_features(
      &strs(&["nixos-test"]),
      &strs(&["kvm", "nixos-test"]),
      &strs(&["kvm"]),
    ));
  }
}
