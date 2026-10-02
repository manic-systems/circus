//! The running state should mean a build already holds execution capacity.

use std::{
  cmp::Ordering,
  collections::HashSet,
  path::Path,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering as AtomicOrdering},
  },
  time::{Duration, Instant},
};

use BuilderSchedulingStrategy::{
  CpuCoreCountWithSpeedFactor,
  Dynamic,
  SpeedFactorOnly,
};
use circus_common::{
  PgPool,
  models::{
    Build,
    BuildKind,
    BuildStatus,
    EFFECT_OUTCOME_UNKNOWN_ERROR,
    Evaluation,
    EvaluationTriggerKind,
  },
  repo,
  repository::RepositoryCoordinates,
};
use circus_config::BuilderSchedulingStrategy;
use tokio::{
  process::Command,
  sync::{OwnedSemaphorePermit, oneshot},
  time::sleep,
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
      EffectContext,
      PresignedUpload,
      SlotGuard,
    },
  },
};

pub enum ExecutionReservation {
  Agent {
    meta:           Arc<AgentMeta>,
    snap:           Box<AgentSnapshot>,
    slot:           SlotGuard,
    effect_context: Option<EffectContext>,
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
pub(crate) fn is_trusted_ref_evaluation(evaluation: &Evaluation) -> bool {
  trusted_ref_context(evaluation).is_some()
}

pub(crate) const UNTRUSTED_EFFECT_ERROR: &str =
  "Effect cancelled: evaluation is not associated with a trusted \
   source-change or interval branch/tag";
pub(crate) const UNIDENTIFIABLE_EFFECT_REPOSITORY_ERROR: &str =
  "Effect cancelled: project repository URL does not identify an owner and \
   repository";

#[must_use]
pub(crate) fn effect_ref_rejection(
  kind: BuildKind,
  evaluation: &Evaluation,
) -> Option<(BuildStatus, &'static str)> {
  (kind.is_effect() && !is_trusted_ref_evaluation(evaluation))
    .then_some((BuildStatus::Cancelled, UNTRUSTED_EFFECT_ERROR))
}

#[must_use]
pub(crate) fn effect_repository_rejection(
  kind: BuildKind,
  repository_url: &str,
) -> Option<(BuildStatus, &'static str)> {
  (kind.is_effect() && repository_url.parse::<RepositoryCoordinates>().is_err())
    .then_some((
      BuildStatus::Cancelled,
      UNIDENTIFIABLE_EFFECT_REPOSITORY_ERROR,
    ))
}

struct TrustedRefContext {
  branch:            String,
  tag:               String,
  is_default_branch: bool,
}

fn trusted_ref_context(evaluation: &Evaluation) -> Option<TrustedRefContext> {
  if evaluation.pr_number.is_some()
    || !matches!(
      evaluation.trigger_kind,
      EvaluationTriggerKind::SourceChange | EvaluationTriggerKind::Interval
    )
  {
    return None;
  }

  if let Some(tag) = evaluation
    .pr_action
    .as_deref()
    .and_then(|action| action.strip_prefix("tag:"))
    .filter(|tag| !tag.is_empty())
  {
    return Some(TrustedRefContext {
      branch:            String::new(),
      tag:               tag.to_owned(),
      is_default_branch: false,
    });
  }
  if evaluation.pr_action.is_some() {
    return None;
  }

  let branch = evaluation
    .pr_head_branch
    .as_deref()
    .filter(|branch| !branch.is_empty())
    .map(|branch| branch.strip_prefix("refs/heads/").unwrap_or(branch))?;
  let attested_default_branch = evaluation
    .pr_base_branch
    .as_deref()
    .map(|branch| branch.strip_prefix("refs/heads/").unwrap_or(branch));

  Some(TrustedRefContext {
    branch:            branch.to_owned(),
    tag:               String::new(),
    is_default_branch: attested_default_branch == Some(branch),
  })
}

struct TrustedBuildContext {
  repository: Option<String>,
  effect:     Option<EffectContext>,
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
  let ref_context = trusted_ref_context(&evaluation)?;
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
  let repository = project.repository_url.parse::<RepositoryCoordinates>().ok();
  Some(TrustedBuildContext {
    repository: github_repository_slug(&project.repository_url),
    effect:     repository.map(|repository| {
      EffectContext {
        project_id:        project.id.to_string(),
        project_path:      repository.project_path,
        owner:             repository.owner,
        repo:              repository.repo,
        branch:            ref_context.branch,
        tag:               ref_context.tag,
        is_default_branch: ref_context.is_default_branch,
      }
    }),
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
    .is_some_and(|repo| trusted.repository.as_deref() == Some(repo))
}

const fn candidate_allowed_for_effect(agent: &AgentSnapshot) -> bool {
  agent.effects && !agent.ephemeral
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

pub enum AgentRunOutcome {
  Completed(BuildResult),
  VenueLost,
  EffectQuarantined,
}

struct AbortOnDrop(Option<oneshot::Sender<()>>);

impl AbortOnDrop {
  const fn new(sender: oneshot::Sender<()>) -> Self {
    Self(Some(sender))
  }

  fn disarm(&mut self) {
    self.0.take();
  }
}

/// Shared handoff marker between the worker task and the RPC connection task.
#[derive(Clone, Default)]
pub struct AgentHandoff(Arc<AtomicBool>);

impl AgentHandoff {
  fn mark_handed_off(&self) {
    self.0.store(true, AtomicOrdering::Release);
  }

  #[must_use]
  pub fn is_handed_off(&self) -> bool {
    self.0.load(AtomicOrdering::Acquire)
  }
}

impl Drop for AbortOnDrop {
  fn drop(&mut self) {
    if let Some(sender) = self.0.take() {
      let _ = sender.send(());
    }
  }
}

#[must_use]
pub(crate) const fn quarantine_on_agent_disconnect(kind: BuildKind) -> bool {
  kind.is_effect()
}

#[must_use]
pub(crate) const fn non_agent_execution_allowed(kind: BuildKind) -> bool {
  !kind.is_effect()
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
    && let Some((meta, snap, slot, effect_context)) =
      select_and_reserve_agent(ctx, build, system).await
  {
    return Some(ExecutionReservation::Agent {
      meta,
      snap: Box::new(snap),
      slot,
      effect_context,
    });
  }

  // Effects require opted-in agents because runner and SSH builders lack
  // secrets and sandboxing.
  if !non_agent_execution_allowed(build.kind) {
    tracing::debug!(
      build_id = %build.id,
      "no effect-capable agent; leaving effect pending"
    );
    return None;
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
) -> Option<(
  Arc<AgentMeta>,
  AgentSnapshot,
  SlotGuard,
  Option<EffectContext>,
)> {
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
  if build.kind.is_effect() {
    candidates.retain(|(_, snap)| candidate_allowed_for_effect(snap));
  }
  if candidates.is_empty() {
    return None;
  }

  let needs_trusted_context = build.kind.is_effect()
    || candidates
      .iter()
      .any(|(_, snap)| snap.requires_trusted_ref());
  let trusted = if needs_trusted_context {
    let trusted = trusted_build_context(&ctx.pool, build).await;
    if build.kind.is_effect()
      && trusted
        .as_ref()
        .and_then(|context| context.effect.as_ref())
        .is_none()
    {
      tracing::warn!(
        build_id = %build.id,
        evaluation_id = %build.evaluation_id,
        "refusing effect without a trusted, fully identified ref context"
      );
      return None;
    }
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
    trusted
  } else {
    None
  };

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

  // A build back in the queue already failed or vanished on that agent.
  let previous = build.agent_machine_id;
  eligible.sort_by(|a, b| {
    let sa = a.1.contended_surplus(build.scheduling_features(), &demand);
    let sb = b.1.contended_surplus(build.scheduling_features(), &demand);
    (Some(a.1.machine_id) == previous)
      .cmp(&(Some(b.1.machine_id) == previous))
      .then_with(|| sa.cmp(&sb))
      .then_with(|| strategy_order(&ctx.scheduling_strategy, &a.1, &b.1))
      .then_with(|| a.1.machine_id.cmp(&b.1.machine_id))
  });

  let effect_context = trusted.and_then(|context| context.effect);
  eligible.into_iter().find_map(|(meta, snap)| {
    meta
      .try_acquire_slot()
      .map(|slot| (meta, snap, slot, effect_context.clone()))
  })
}

pub async fn run_on_agent(
  meta: &Arc<AgentMeta>,
  snap: &AgentSnapshot,
  slot: SlotGuard,
  effect_context: Option<EffectContext>,
  pool: &PgPool,
  build: &Build,
  drv_path: &str,
  live_log_path: &Path,
  opts: &AgentDispatch<'_>,
  handoff: &AgentHandoff,
) -> AgentRunOutcome {
  let (tx, rx) = oneshot::channel();
  let (abort_tx, abort_rx) = oneshot::channel();
  let presigned_upload =
    (opts.cache_upload_enabled_s3 && effect_context.is_none()).then(|| {
      PresignedUpload {
        compression:                opts.cache_upload_compression.to_owned(),
        fail_build_on_upload_error: opts.fail_build_on_upload_error,
      }
    });
  let cache_upload_handled = presigned_upload.is_some();

  let cmd = DispatchCommand {
    build_id: build.id,
    attempt: build.retry_count,
    drv_path: drv_path.to_owned(),
    effect: effect_context,
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
    abort: abort_rx,
    reservation: slot,
    completion: tx,
  };
  meta.active_builds.write().insert(build.id);
  if meta.tx.send(cmd).is_err() {
    meta.active_builds.write().remove(&build.id);
    tracing::warn!(name = %snap.name, "agent channel closed, falling back");
    return AgentRunOutcome::VenueLost;
  }
  handoff.mark_handed_off();
  let mut abort_on_drop = AbortOnDrop::new(abort_tx);

  if let Err(e) = repo::builder_sessions::touch(pool, meta.machine_id).await {
    tracing::debug!(name = %snap.name, "builder_sessions touch failed: {e}");
  }
  tracing::info!(build_id = %build.id, agent = %snap.name, "dispatched to agent");

  let result = |success, exit_code, stderr: String, output_paths| {
    BuildResult {
      success,
      exit_code: Some(exit_code),
      stdout: String::new(),
      stderr,
      output_paths,
      cache_upload_handled,
    }
  };

  let outcome = rx.await;
  abort_on_drop.disarm();
  match outcome {
    Ok(DispatchResult::Succeeded { error_message }) => {
      if build.kind.is_effect() {
        return AgentRunOutcome::Completed(result(
          true,
          0,
          error_message.unwrap_or_default(),
          Vec::new(),
        ));
      }
      let outputs = match try_read_drv_outputs(drv_path).await {
        Ok(outputs) if !outputs.is_empty() => outputs,
        Ok(_) => {
          return AgentRunOutcome::Completed(result(
            false,
            1,
            format!("{drv_path} has no queryable outputs on the runner"),
            Vec::new(),
          ));
        },
        Err(e) => {
          return AgentRunOutcome::Completed(result(
            false,
            1,
            format!("could not read the outputs of {drv_path}: {e}"),
            Vec::new(),
          ));
        },
      };
      if !opts.cache_upload_enabled_s3 {
        match invalid_store_paths(&outputs).await {
          Ok(missing) if missing.is_empty() => {},
          Ok(missing) => {
            return AgentRunOutcome::Completed(result(
              false,
              1,
              format!(
                "build succeeded on {} but its outputs never reached the \
                 runner: {}",
                snap.name,
                missing.join(" ")
              ),
              Vec::new(),
            ));
          },
          Err(e) => {
            return AgentRunOutcome::Completed(result(
              false,
              1,
              format!("could not verify the outputs on the runner: {e}"),
              Vec::new(),
            ));
          },
        }
      }
      AgentRunOutcome::Completed(result(
        true,
        0,
        error_message.unwrap_or_default(),
        outputs,
      ))
    },
    Ok(DispatchResult::Failed(error_message)) => {
      AgentRunOutcome::Completed(result(false, 1, error_message, Vec::new()))
    },
    Ok(DispatchResult::TimedOut) => {
      AgentRunOutcome::Completed(result(
        false,
        124,
        "build timed out".into(),
        Vec::new(),
      ))
    },
    Ok(DispatchResult::Aborted) => {
      AgentRunOutcome::Completed(result(
        false,
        130,
        "build aborted".into(),
        Vec::new(),
      ))
    },
    Ok(DispatchResult::OomKilled(error_message)) => {
      AgentRunOutcome::Completed(result(false, -9, error_message, Vec::new()))
    },
    Ok(DispatchResult::Refused(reason))
      if !quarantine_on_agent_disconnect(build.kind) =>
    {
      tracing::warn!(
        name = %snap.name,
        reason,
        "agent refused the assignment; retrying after {AGENT_REFUSAL_BACKOFF:?}"
      );
      sleep(AGENT_REFUSAL_BACKOFF).await;
      AgentRunOutcome::VenueLost
    },
    Ok(DispatchResult::Disconnected | DispatchResult::Refused(_)) | Err(_) => {
      if quarantine_on_agent_disconnect(build.kind) {
        if let Err(e) = repo::builds::quarantine_effect(
          pool,
          build.id,
          snap.machine_id,
          build.retry_count,
          EFFECT_OUTCOME_UNKNOWN_ERROR,
        )
        .await
        {
          tracing::warn!(
            build_id = %build.id,
            "failed to record disconnected effect quarantine: {e}"
          );
        }
        tracing::warn!(
          build_id = %build.id,
          name = %snap.name,
          "agent disconnected during effect; quarantining running row"
        );
        AgentRunOutcome::EffectQuarantined
      } else {
        tracing::warn!(
          name = %snap.name,
          "agent disconnected mid-build; falling back"
        );
        AgentRunOutcome::VenueLost
      }
    },
  }
}

/// A refusal like "already running" only clears once the agent finishes.
const AGENT_REFUSAL_BACKOFF: Duration = Duration::from_secs(30);

/// Output transfer from the agent is best-effort, so a lost closure must
/// not read as success.
pub(crate) async fn invalid_store_paths(
  paths: &[String],
) -> Result<Vec<String>, String> {
  let mut invalid = Vec::new();
  // A whole closure on one command line can exceed ARG_MAX.
  for batch in paths.chunks(1024) {
    let out = Command::new("nix-store")
      .args(["--check-validity", "--print-invalid"])
      .args(batch)
      .output()
      .await
      .map_err(|e| format!("nix-store --check-validity: {e}"))?;
    if !out.status.success() {
      return Err(format!(
        "nix-store --check-validity exited with {}: {}",
        out.status,
        String::from_utf8_lossy(&out.stderr).trim()
      ));
    }
    invalid.extend(
      String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned),
    );
  }
  Ok(invalid)
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
    BuildKind,
    BuildStatus,
    EvaluationStatus,
  };
  use uuid::Uuid;

  use super::{
    AbortOnDrop,
    AgentHandoff,
    AgentSnapshot,
    UNIDENTIFIABLE_EFFECT_REPOSITORY_ERROR,
    UNTRUSTED_EFFECT_ERROR,
    candidate_allowed_for_effect,
    candidate_allowed_for_trusted_build,
    effect_ref_rejection,
    effect_repository_rejection,
    github_repository_slug,
    is_trusted_ref_evaluation,
    non_agent_execution_allowed,
    quarantine_on_agent_disconnect,
    supports_required_features,
    trusted_ref_context,
  };
  use crate::rpc::pool::HeartbeatSnapshot;

  fn strs(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
  }

  fn demand(values: &[&str]) -> HashSet<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
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

  fn attested_evaluation(
    trigger_kind: circus_common::models::EvaluationTriggerKind,
    branch: Option<&str>,
    default_branch: Option<&str>,
  ) -> circus_common::models::Evaluation {
    let mut evaluation = evaluation(trigger_kind, None);
    evaluation.pr_head_branch = branch.map(str::to_owned);
    evaluation.pr_base_branch = default_branch.map(str::to_owned);
    evaluation
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
      effects: false,
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
  fn trusted_ref_uses_immutable_evaluation_provenance() {
    use circus_common::models::EvaluationTriggerKind::{
      Interval,
      Manual,
      SourceChange,
    };
    let fixed_evaluation =
      attested_evaluation(SourceChange, Some("main"), Some("main"));
    assert!(is_trusted_ref_evaluation(&fixed_evaluation));
    assert!(is_trusted_ref_evaluation(&attested_evaluation(
      Interval,
      Some("main"),
      Some("main"),
    )));
    assert!(!is_trusted_ref_evaluation(&attested_evaluation(
      Manual,
      Some("main"),
      Some("main"),
    )));
    let fixed = trusted_ref_context(&fixed_evaluation)
      .expect("attested polling branch is trusted");
    assert_eq!(fixed.branch, "main");
    assert!(fixed.is_default_branch);
    assert!(!is_trusted_ref_evaluation(&evaluation(SourceChange, None,)));
    assert!(!is_trusted_ref_evaluation(&evaluation(
      SourceChange,
      Some("feature"),
    )));

    let matched_evaluation =
      attested_evaluation(SourceChange, Some("release/1.0"), None);
    let matched = trusted_ref_context(&matched_evaluation)
      .expect("matched branch is trusted");
    assert_eq!(matched.branch, "release/1.0");
    assert!(matched.tag.is_empty());
    assert!(!matched.is_default_branch);

    let mut tag_evaluation = evaluation(SourceChange, None);
    tag_evaluation.pr_action = Some("tag:v1.0".into());
    let tag =
      trusted_ref_context(&tag_evaluation).expect("attested tag is trusted");
    assert!(tag.branch.is_empty());
    assert_eq!(tag.tag, "v1.0");
    assert!(!tag.is_default_branch);
    tag_evaluation.pr_action = Some("not-a-ref-attestation".into());
    assert!(trusted_ref_context(&tag_evaluation).is_none());
  }

  #[test]
  fn untrusted_effects_cancel_while_trusted_effects_and_builds_continue() {
    use circus_common::models::EvaluationTriggerKind::{Manual, SourceChange};

    let pr = evaluation(SourceChange, Some("feature"));
    assert_eq!(
      effect_ref_rejection(BuildKind::Effect, &pr),
      Some((BuildStatus::Cancelled, UNTRUSTED_EFFECT_ERROR))
    );
    assert_eq!(
      effect_ref_rejection(
        BuildKind::Effect,
        &attested_evaluation(SourceChange, Some("main"), Some("main")),
      ),
      None
    );
    assert_eq!(effect_ref_rejection(BuildKind::Build, &pr), None);
    assert_eq!(
      effect_ref_rejection(BuildKind::Effect, &evaluation(Manual, None)),
      Some((BuildStatus::Cancelled, UNTRUSTED_EFFECT_ERROR))
    );
  }

  #[test]
  fn effects_require_a_persistent_opted_in_agent() {
    let mut persistent = agent_snapshot(false, AuthKind::Token, None);
    persistent.effects = true;
    let mut ephemeral = agent_snapshot(true, AuthKind::Token, None);
    ephemeral.effects = true;
    let not_opted_in = agent_snapshot(false, AuthKind::Token, None);

    assert!(candidate_allowed_for_effect(&persistent));
    assert!(!candidate_allowed_for_effect(&ephemeral));
    assert!(!candidate_allowed_for_effect(&not_opted_in));
  }

  #[test]
  fn effects_never_fall_back_to_local_or_ssh_execution() {
    assert!(non_agent_execution_allowed(BuildKind::Build));
    assert!(!non_agent_execution_allowed(BuildKind::Effect));
  }

  #[test]
  fn only_effect_disconnects_quarantine_the_running_row() {
    assert!(quarantine_on_agent_disconnect(BuildKind::Effect));
    assert!(!quarantine_on_agent_disconnect(BuildKind::Build));
  }

  #[tokio::test]
  async fn dropping_an_inflight_agent_effect_requests_remote_abort() {
    let (abort_tx, abort_rx) = tokio::sync::oneshot::channel();
    let guard = AbortOnDrop::new(abort_tx);
    drop(guard);
    assert_eq!(abort_rx.await, Ok(()));
  }

  #[test]
  fn agent_handoff_marker_is_monotonic() {
    let handoff = AgentHandoff::default();
    let observer = handoff.clone();
    assert!(!handoff.is_handed_off());
    handoff.mark_handed_off();
    assert!(handoff.is_handed_off());
    assert!(observer.is_handed_off());
  }

  #[test]
  fn oidc_agent_must_match_project_repository() {
    let trusted = super::TrustedBuildContext {
      repository: Some("owner/repo".into()),
      effect:     None,
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

  #[test]
  fn effects_need_an_identifiable_repository() {
    assert_eq!(
      effect_repository_rejection(BuildKind::Effect, "/srv/repos/infra"),
      Some((
        BuildStatus::Cancelled,
        UNIDENTIFIABLE_EFFECT_REPOSITORY_ERROR,
      ))
    );
    assert_eq!(
      effect_repository_rejection(BuildKind::Build, "/srv/repos/infra"),
      None
    );
  }
}
