//! The running state should mean a build already holds execution capacity.

use std::{
  cmp::Ordering,
  collections::HashSet,
  path::Path,
  sync::Arc,
  time::{Duration, Instant},
};

use BuilderSchedulingStrategy::{
  CpuCoreCountWithSpeedFactor,
  Dynamic,
  SpeedFactorOnly,
};
use circus_common::{
  PgPool,
  models::{Build, Evaluation, EvaluationTriggerKind, Jobset},
  repo,
};
use circus_config::BuilderSchedulingStrategy;
use tokio::{
  process::Command,
  sync::{OwnedSemaphorePermit, oneshot},
};

use crate::{
  builder::BuildResult,
  context::BuildContext,
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
    snap: Box<AgentSnapshot>,
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

#[must_use]
pub(crate) fn is_trusted_ref_evaluation(
  evaluation: &Evaluation,
  jobset: &Jobset,
) -> bool {
  if jobset.branch.as_deref().is_none_or(str::is_empty) {
    return false;
  }
  if evaluation.pr_number.is_some()
    || evaluation.pr_head_branch.is_some()
    || evaluation.pr_base_branch.is_some()
  {
    return false;
  }
  matches!(
    evaluation.trigger_kind,
    EvaluationTriggerKind::SourceChange
      | EvaluationTriggerKind::Manual
      | EvaluationTriggerKind::Interval
  )
}

struct TrustedBuildContext {
  repository: Option<String>,
}

async fn trusted_build_context(
  pool: &PgPool,
  build: &Build,
) -> Option<TrustedBuildContext> {
  let evaluation = match repo::evaluations::get(pool, build.evaluation_id).await
  {
    Ok(evaluation) => evaluation,
    Err(e) => {
      tracing::warn!(
        build_id = %build.id,
        evaluation_id = %build.evaluation_id,
        "failed to load evaluation for trusted-ref scheduling: {e}"
      );
      return None;
    },
  };
  let jobset = match repo::jobsets::get(pool, evaluation.jobset_id).await {
    Ok(jobset) => jobset,
    Err(e) => {
      tracing::warn!(
        build_id = %build.id,
        jobset_id = %evaluation.jobset_id,
        "failed to load jobset for trusted-ref scheduling: {e}"
      );
      return None;
    },
  };
  if !is_trusted_ref_evaluation(&evaluation, &jobset) {
    return None;
  }
  let project = match repo::projects::get(pool, jobset.project_id).await {
    Ok(project) => project,
    Err(e) => {
      tracing::warn!(
        build_id = %build.id,
        project_id = %jobset.project_id,
        "failed to load project for trusted-ref scheduling: {e}"
      );
      return None;
    },
  };
  Some(TrustedBuildContext {
    repository: github_repository_slug(&project.repository_url),
  })
}

pub(crate) async fn trusted_build_github_repository(
  pool: &PgPool,
  build: &Build,
) -> Option<Option<String>> {
  trusted_build_context(pool, build)
    .await
    .map(|context| context.repository)
}

fn candidate_allowed_for_trusted_build(
  agent: &AgentSnapshot,
  trusted: Option<&TrustedBuildContext>,
) -> bool {
  if !agent.requires_trusted_ref() {
    return true;
  }
  let Some(trusted) = trusted else {
    return false;
  };
  // OIDC agents are pinned to their token repo
  agent
    .oidc_repository
    .as_deref()
    .is_none_or(|repo| trusted.repository.as_deref() == Some(repo))
}

#[must_use]
pub(crate) fn github_repository_slug(url: &str) -> Option<String> {
  let url = url.trim().trim_end_matches(".git");
  if let Some(rest) = url.strip_prefix("git@github.com:") {
    return owner_repo_from_path(rest);
  }
  if let Ok(parsed) = url::Url::parse(url)
    && parsed.host_str() == Some("github.com")
  {
    return owner_repo_from_path(parsed.path().trim_start_matches('/'));
  }
  None
}

fn owner_repo_from_path(path: &str) -> Option<String> {
  let mut parts = path.split('/').filter(|part| !part.is_empty());
  let owner = parts.next()?;
  let repo = parts.next()?;
  if parts.next().is_some() {
    return None;
  }
  Some(format!("{owner}/{repo}"))
}

/// Load-based ordering for the configured strategy, used as the tie-break once
/// builders are ranked by contended surplus.
///
/// # Returns
///
/// Returns [`Ordering::Less`] when `a` is the better choice.
fn strategy_order(
  strategy: &BuilderSchedulingStrategy,
  a: &AgentSnapshot,
  b: &AgentSnapshot,
) -> Ordering {
  match strategy {
    SpeedFactorOnly => {
      b.speed_factor
        .partial_cmp(&a.speed_factor)
        .unwrap_or(Ordering::Equal)
    },
    CpuCoreCountWithSpeedFactor => {
      let av = a.cpu_count as f32 * a.speed_factor;
      let bv = b.cpu_count as f32 * b.speed_factor;
      bv.partial_cmp(&av).unwrap_or(Ordering::Equal)
    },
    Dynamic => {
      let free = |s: &AgentSnapshot| -> f32 {
        s.max_jobs.saturating_sub(s.current_jobs) as f32 * s.speed_factor
      };
      free(b).partial_cmp(&free(a)).unwrap_or(Ordering::Equal)
    },
  }
}

pub struct AgentDispatch<'a> {
  pub timeout:                    Duration,
  pub max_silent_time:            Duration,
  pub extra_nix_args:             &'a [String],
  pub cache_upload_enabled_s3:    bool,
  pub cache_upload_compression:   &'a str,
  pub fail_build_on_upload_error: bool,
}

/// Reserve capacity before the build is claimed as running. [`None`] means
/// there is no capable venue.
///
/// # Panics
///
/// Only if the worker semaphore has been closed, which never happens during
/// normal operation.
pub async fn reserve_venue(
  ctx: &BuildContext,
  build: &Build,
  system: Option<&str>,
) -> Option<ExecutionReservation> {
  if let Some(system) = system
    && let Some((meta, snap, slot)) =
      select_and_reserve_agent(ctx, build, system).await
  {
    return Some(ExecutionReservation::Agent {
      meta,
      snap: Box::new(snap),
      slot,
    });
  }

  let features = build.scheduling_features();
  if !ctx.runner_caps.supports(system, features) {
    tracing::debug!(
      build_id = %build.id,
      ?features,
      "no capable venue; leaving build pending"
    );
    return None;
  }

  #[expect(
    clippy::expect_used,
    reason = "the worker semaphore is never closed, so acquire never errors"
  )]
  let permit = Arc::clone(&ctx.worker_semaphore)
    .acquire_owned()
    .await
    .expect("worker semaphore is never closed");
  Some(ExecutionReservation::Runner(permit))
}

async fn select_and_reserve_agent(
  ctx: &BuildContext,
  build: &Build,
  system: &str,
) -> Option<(Arc<AgentMeta>, AgentSnapshot, SlotGuard)> {
  let mut candidates = ctx.agent_pool.candidates_for(system);
  if candidates.is_empty() {
    return None;
  }

  // Missing or stale heartbeats are treated as unknown.
  let cutoff = Instant::now().checked_sub(ctx.heartbeat_ttl);
  if let Some(t) = ctx.psi_threshold {
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

  candidates
    .retain(|(_, snap)| snap.supports_features(build.scheduling_features()));
  if candidates.is_empty() {
    return None;
  }

  if candidates
    .iter()
    .any(|(_, snap)| snap.requires_trusted_ref())
  {
    let trusted = trusted_build_context(&ctx.pool, build).await;
    candidates.retain(|(_, snap)| {
      candidate_allowed_for_trusted_build(snap, trusted.as_ref())
    });
    if candidates.is_empty() {
      tracing::debug!(
        build_id = %build.id,
        "skipping ephemeral/OIDC agents for untrusted ref"
      );
      return None;
    }
  }

  let mut eligible = Vec::with_capacity(candidates.len());
  for candidate in candidates {
    match repo::builder_sessions::is_schedulable(
      &ctx.pool,
      candidate.0.machine_id,
    )
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

  if eligible.is_empty() {
    return None;
  }

  // Capability-preserving order: prefer builders that waste the fewest
  // currently-contended capabilities on this build, so a versatile builder is
  // kept free for the queued work that actually needs it.
  let demand = repo::builds::pending_feature_demand(&ctx.pool, system)
    .await
    .unwrap_or_else(|e| {
      tracing::warn!(
        "pending_feature_demand failed, falling back to load-only ordering: \
         {e}"
      );
      HashSet::new()
    });

  eligible.sort_by(|a, b| {
    let sa = a.1.contended_surplus(build.scheduling_features(), &demand);
    let sb = b.1.contended_surplus(build.scheduling_features(), &demand);
    sa.cmp(&sb)
      .then_with(|| strategy_order(&ctx.scheduling_strategy, &a.1, &b.1))
      .then_with(|| a.1.machine_id.cmp(&b.1.machine_id))
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

  if let Err(e) = repo::builder_sessions::touch(pool, meta.machine_id).await {
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
      cache_upload_handled: opts.cache_upload_enabled_s3,
    }
  };

  match rx.await {
    Ok(DispatchResult::Succeeded { error_message }) => {
      let outputs = read_drv_outputs(drv_path).await;
      Some(result(true, 0, error_message.unwrap_or_default(), outputs))
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
    Ok(DispatchResult::OomKilled(error_message)) => {
      Some(result(false, -9, error_message, Vec::new()))
    },
    Ok(DispatchResult::Disconnected) | Err(_) => {
      tracing::warn!(name = %snap.name, "agent disconnected mid-build; falling back");
      None
    },
  }
}

pub(crate) async fn read_drv_outputs(drv_path: &str) -> Vec<String> {
  try_read_drv_outputs(drv_path).await.unwrap_or_default()
}

/// Every store path the derivation needs, including its input drvs.
pub(crate) async fn drv_requisites(
  drv_path: &str,
) -> color_eyre::Result<Vec<String>> {
  let out = Command::new("nix-store")
    .args(["--query", "--requisites", drv_path])
    .output()
    .await?;
  if !out.status.success() {
    return Err(color_eyre::eyre::eyre!(
      "nix-store --query --requisites {drv_path} exited with {}",
      out.status
    ));
  }
  Ok(
    String::from_utf8_lossy(&out.stdout)
      .lines()
      .map(|s| s.trim().to_owned())
      .filter(|s| !s.is_empty())
      .collect(),
  )
}

pub(crate) async fn try_read_drv_outputs(
  drv_path: &str,
) -> color_eyre::Result<Vec<String>> {
  let out = Command::new("nix-store")
    .args(["--query", "--outputs", drv_path])
    .output()
    .await?;
  if !out.status.success() {
    return Err(color_eyre::eyre::eyre!(
      "nix-store --query --outputs {drv_path} exited with {}",
      out.status
    ));
  }
  Ok(
    String::from_utf8_lossy(&out.stdout)
      .lines()
      .map(|s| s.trim().to_owned())
      .filter(|s| !s.is_empty())
      .collect(),
  )
}

#[cfg(test)]
mod tests {
  use std::collections::HashSet;

  use chrono::Utc;
  use circus_common::models::{
    AuthKind,
    EvaluationStatus,
    JobsetState,
    JobsetTriggerMode,
  };
  use uuid::Uuid;

  use super::{
    AgentSnapshot,
    candidate_allowed_for_trusted_build,
    github_repository_slug,
    is_trusted_ref_evaluation,
    supports_required_features,
  };
  use crate::rpc::pool::HeartbeatSnapshot;

  fn strs(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
  }

  fn demand(values: &[&str]) -> HashSet<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
  }

  fn jobset(branch: Option<&str>) -> circus_common::models::Jobset {
    let now = Utc::now();
    circus_common::models::Jobset {
      id:                Uuid::new_v4(),
      project_id:        Uuid::new_v4(),
      name:              "packages".into(),
      nix_expression:    "packages".into(),
      enabled:           true,
      flake_mode:        true,
      check_interval:    60,
      trigger_mode:      JobsetTriggerMode::SourceChange,
      branch:            branch.map(str::to_owned),
      branch_pattern:    None,
      tag_pattern:       None,
      scheduling_shares: 100,
      created_at:        now,
      updated_at:        now,
      state:             JobsetState::Enabled,
      last_checked_at:   None,
      keep_nr:           3,
      systems:           None,
      only_build_latest: false,
      path_filters:      Vec::new(),
    }
  }

  fn evaluation(
    trigger_kind: circus_common::models::EvaluationTriggerKind,
    pr_head_branch: Option<&str>,
  ) -> circus_common::models::Evaluation {
    circus_common::models::Evaluation {
      id: Uuid::new_v4(),
      jobset_id: Uuid::new_v4(),
      commit_hash: "0123456789012345678901234567890123456789".into(),
      evaluation_time: Utc::now(),
      status: EvaluationStatus::Completed,
      error_message: None,
      inputs_hash: None,
      trigger_kind,
      hidden: false,
      pr_number: pr_head_branch.map(|_| 1),
      pr_head_branch: pr_head_branch.map(str::to_owned),
      pr_base_branch: pr_head_branch.map(|_| "main".to_owned()),
      pr_action: None,
      source_scope: None,
      superseded_by: None,
      source_base_commit: None,
    }
  }

  fn agent_snapshot(
    ephemeral: bool,
    auth_kind: circus_common::models::AuthKind,
    oidc_repository: Option<&str>,
  ) -> AgentSnapshot {
    AgentSnapshot {
      machine_id: Uuid::new_v4(),
      name: "agent".into(),
      systems: vec!["x86_64-linux".into()],
      supported_features: Vec::new(),
      mandatory_features: Vec::new(),
      speed_factor: 1.0,
      cpu_count: 1,
      max_jobs: 1,
      current_jobs: 0,
      ephemeral,
      auth_kind,
      oidc_repository: oidc_repository.map(str::to_owned),
      oidc_subject: None,
      heartbeat: HeartbeatSnapshot::default(),
    }
  }

  #[test]
  fn no_contention_scores_zero_for_every_builder() {
    // Nothing queued demands a feature, so ordering must fall back entirely to
    // the load strategy (every builder scores 0).
    let empty = demand(&[]);
    let mut versatile = agent_snapshot(false, AuthKind::Token, None);
    versatile.supported_features = strs(&["kvm", "big-parallel"]);
    let plain = agent_snapshot(false, AuthKind::Token, None);
    assert_eq!(versatile.contended_surplus(&strs(&[]), &empty), 0);
    assert_eq!(plain.contended_surplus(&strs(&[]), &empty), 0);
  }

  #[test]
  fn fungible_build_is_penalised_on_a_contended_builder() {
    // A plain build, while a kvm build is queued
    let d = demand(&["kvm"]);
    let mut kvm = agent_snapshot(false, AuthKind::Token, None);
    kvm.supported_features = strs(&["kvm"]);
    let plain = agent_snapshot(false, AuthKind::Token, None);
    assert_eq!(kvm.contended_surplus(&strs(&[]), &d), 1);
    assert_eq!(plain.contended_surplus(&strs(&[]), &d), 0);
  }

  #[test]
  fn a_builds_own_required_feature_is_never_surplus() {
    // The kvm build itself belongs on the kvm builder, so kvm must not count
    // against it even though kvm is in demand.
    let d = demand(&["kvm"]);
    let mut kvm = agent_snapshot(false, AuthKind::Token, None);
    kvm.supported_features = strs(&["kvm"]);
    assert_eq!(kvm.contended_surplus(&strs(&["kvm"]), &d), 0);
  }

  #[test]
  fn only_demanded_features_count_not_noise() {
    // `benchmark` is advertised but nothing demands it, so it must not inflate
    // surplus, only the demanded `uid-range` does.
    let d = demand(&["uid-range"]);
    let mut agent = agent_snapshot(false, AuthKind::Token, None);
    agent.supported_features =
      strs(&["benchmark", "big-parallel", "uid-range"]);
    assert_eq!(agent.contended_surplus(&strs(&[]), &d), 1);
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

  #[test]
  fn trusted_ref_requires_concrete_jobset_branch_and_not_pr() {
    use circus_common::models::EvaluationTriggerKind::SourceChange;
    assert!(is_trusted_ref_evaluation(
      &evaluation(SourceChange, None),
      &jobset(Some("main"))
    ));
    assert!(!is_trusted_ref_evaluation(
      &evaluation(SourceChange, None),
      &jobset(None)
    ));
    assert!(!is_trusted_ref_evaluation(
      &evaluation(SourceChange, Some("feature")),
      &jobset(Some("main"))
    ));
  }

  #[test]
  fn oidc_agent_must_match_project_repository() {
    let trusted = super::TrustedBuildContext {
      repository: Some("owner/repo".into()),
    };
    let matching = agent_snapshot(true, AuthKind::Oidc, Some("owner/repo"));
    let other_repo = agent_snapshot(true, AuthKind::Oidc, Some("owner/other"));
    let token_ephemeral = agent_snapshot(true, AuthKind::Token, None);
    let persistent = agent_snapshot(false, AuthKind::Token, None);

    assert!(candidate_allowed_for_trusted_build(
      &matching,
      Some(&trusted)
    ));
    assert!(!candidate_allowed_for_trusted_build(
      &other_repo,
      Some(&trusted)
    ));
    assert!(candidate_allowed_for_trusted_build(
      &token_ephemeral,
      Some(&trusted)
    ));
    assert!(candidate_allowed_for_trusted_build(&persistent, None));
  }

  #[test]
  fn github_slug_accepts_https_and_ssh_urls() {
    assert_eq!(
      github_repository_slug("https://github.com/owner/repo.git").as_deref(),
      Some("owner/repo")
    );
    assert_eq!(
      github_repository_slug("git@github.com:owner/repo.git").as_deref(),
      Some("owner/repo")
    );
    assert_eq!(
      github_repository_slug("https://example.com/owner/repo"),
      None
    );
  }
}
