//! In-memory registry of connected agents.
//!
//! The pool is the runner's source of truth for "who can I send work to
//! right now". A row in `builder_sessions` survives across restarts, but a
//! cold row is not useful for dispatch: only a live entry here represents
//! an agent we can currently call.
//!
//! Cross-thread design: capnp-rpc capabilities are `!Send` (they're
//! `Rc`-backed). The scheduler runs on the multi-threaded runtime, the
//! RPC server runs in its own `LocalSet`. We bridge the two with a
//! per-agent `tokio::sync::mpsc` channel: the scheduler pushes a
//! [`DispatchCommand`], the per-connection task pops it off and invokes
//! the local `Builder` capability. The capability never leaves the
//! connection task.

use std::{
  collections::{HashMap, HashSet},
  sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
  },
  time::Instant,
};

use parking_lot::RwLock;
use tokio::sync::mpsc;
use uuid::Uuid;

/// Upper bound on an agent's advertised `max_jobs`.
pub const MAX_AGENT_MAX_JOBS: u32 = 670;

/// One command queued from the scheduler to a connected agent.
pub struct DispatchCommand {
  pub build_id:         Uuid,
  pub drv_path:         String,
  pub max_log_size:     u64,
  pub max_silent_time:  u32,
  pub build_timeout:    u32,
  pub extra_args:       Vec<String>,
  pub log_path:         std::path::PathBuf,
  /// `Some(compression)` enables the presigned-upload path: after a
  /// successful build the agent pushes each output's NAR directly to S3
  /// via a presigned URL minted by the runner. `None` disables it; the
  /// runner's own `nix copy --to s3://...` post-build path stays in
  /// charge.
  pub presigned_upload: Option<PresignedUpload>,
  /// Keep the agent slot reservation with the command so failed handoff and
  /// connection-task cleanup use the same release path.
  pub reservation:      SlotGuard,
  /// Completion signal: the per-connection task sends the outcome here
  /// after the agent reports via `ResultSink`. Some scheduler errors are
  /// also surfaced here (queue full, connection closed mid-dispatch).
  pub completion:       tokio::sync::oneshot::Sender<DispatchResult>,
}

#[derive(Debug, Clone)]
pub struct PresignedUpload {
  pub compression:                String,
  pub fail_build_on_upload_error: bool,
}

#[derive(Debug)]
pub enum DispatchResult {
  Succeeded,
  Failed(String),
  TimedOut,
  Aborted,
  /// Agent connection dropped before the result arrived; the caller
  /// should treat this as a transient failure and retry on another
  /// agent.
  Disconnected,
}

/// The metadata side of an agent. This is held in an [`AgentPool`] and shared
/// with the scheduler.
pub struct AgentMeta {
  pub machine_id:         Uuid,
  pub connection_id:      Uuid,
  pub name:               String,
  pub hostname:           String,
  pub systems:            Vec<String>,
  pub supported_features: Vec<String>,
  pub mandatory_features: Vec<String>,
  pub speed_factor:       f32,
  pub cpu_count:          u32,
  pub max_jobs:           u32,
  pub ephemeral:          bool,
  pub auth_kind:          String,
  pub oidc_repository:    Option<String>,
  pub oidc_subject:       Option<String>,

  current_jobs:      Arc<AtomicU32>,
  pub active_builds: RwLock<HashSet<Uuid>>,

  pub heartbeat:     RwLock<HeartbeatSnapshot>,
  pub registered_at: Instant,

  /// Hand-off into the connection task.
  pub tx: mpsc::UnboundedSender<DispatchCommand>,
}

impl AgentMeta {
  /// Build agent metadata from registration data. `current_jobs` starts at
  /// zero and is thereafter mutated only via [`Self::try_acquire_slot`].
  #[must_use]
  #[expect(
    clippy::too_many_arguments,
    reason = "fields come straight from the agent's registration record"
  )]
  pub fn new(
    machine_id: Uuid,
    connection_id: Uuid,
    name: String,
    hostname: String,
    systems: Vec<String>,
    supported_features: Vec<String>,
    mandatory_features: Vec<String>,
    speed_factor: f32,
    cpu_count: u32,
    max_jobs: u32,
    ephemeral: bool,
    auth_kind: String,
    oidc_repository: Option<String>,
    oidc_subject: Option<String>,
    tx: mpsc::UnboundedSender<DispatchCommand>,
  ) -> Self {
    Self {
      machine_id,
      connection_id,
      name,
      hostname,
      systems,
      supported_features,
      mandatory_features,
      speed_factor,
      cpu_count,
      max_jobs,
      ephemeral,
      auth_kind,
      oidc_repository,
      oidc_subject,
      current_jobs: Arc::new(AtomicU32::new(0)),
      active_builds: RwLock::new(HashSet::new()),
      heartbeat: RwLock::new(HeartbeatSnapshot::default()),
      registered_at: Instant::now(),
      tx,
    }
  }

  /// Returns [`None`] when the agent is already at
  /// [`max_jobs`][`Self::max_jobs`].
  ///
  /// The guard moves into [`DispatchCommand`], keeping the slot held until
  /// the send fails or the connection task finishes the build.
  #[must_use]
  pub fn try_acquire_slot(self: &Arc<Self>) -> Option<SlotGuard> {
    let mut cur = self.current_jobs.load(Ordering::Relaxed);
    loop {
      if cur >= self.max_jobs {
        return None;
      }
      match self.current_jobs.compare_exchange_weak(
        cur,
        cur + 1,
        Ordering::AcqRel,
        Ordering::Relaxed,
      ) {
        Ok(_) => {
          return Some(SlotGuard {
            meta: Arc::clone(self),
          });
        },
        Err(actual) => cur = actual,
      }
    }
  }
}

/// Releases one agent build slot when dropped.
pub struct SlotGuard {
  meta: Arc<AgentMeta>,
}

impl Drop for SlotGuard {
  fn drop(&mut self) {
    self.meta.current_jobs.fetch_sub(1, Ordering::AcqRel);
  }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct HeartbeatSnapshot {
  pub last_seen:     Option<Instant>,
  pub load1:         f32,
  pub load5:         f32,
  pub load15:        f32,
  pub cpu_psi_avg10: f32,
  pub mem_psi_avg10: f32,
  pub io_psi_avg10:  f32,
}

/// Cheap clone of the metadata for the scheduler; does not hold the
/// channel sender.
#[derive(Debug, Clone)]
pub struct AgentSnapshot {
  pub machine_id:         Uuid,
  pub name:               String,
  pub systems:            Vec<String>,
  pub supported_features: Vec<String>,
  pub mandatory_features: Vec<String>,
  pub speed_factor:       f32,
  pub cpu_count:          u32,
  pub max_jobs:           u32,
  pub current_jobs:       u32,
  pub ephemeral:          bool,
  pub auth_kind:          String,
  pub oidc_repository:    Option<String>,
  pub oidc_subject:       Option<String>,
  pub heartbeat:          HeartbeatSnapshot,
}

#[derive(Default)]
pub struct AgentPool {
  inner: RwLock<HashMap<Uuid, Arc<AgentMeta>>>,
}

// Hand-rolled to avoid requiring Debug on AgentMeta's mpsc sender.
// Renders only the count and known machine_ids.
impl std::fmt::Debug for AgentPool {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let g = self.inner.read();
    f.debug_struct("AgentPool")
      .field("len", &g.len())
      .field("machine_ids", &g.keys().collect::<Vec<_>>())
      .finish()
  }
}

/// Backwards-compatibility alias: `AgentHandle` was the original name
/// when the pool held the capability directly. The metadata struct now
/// fills the same role from the scheduler's point of view.
pub type AgentHandle = AgentMeta;

impl AgentPool {
  #[must_use]
  pub fn new() -> Arc<Self> {
    Arc::new(Self::default())
  }

  pub fn insert(&self, meta: Arc<AgentMeta>) -> Option<Arc<AgentMeta>> {
    self.inner.write().insert(meta.machine_id, meta)
  }

  pub fn remove(&self, machine_id: &Uuid) -> Option<Arc<AgentMeta>> {
    self.inner.write().remove(machine_id)
  }

  pub fn remove_if_connection(
    &self,
    machine_id: &Uuid,
    connection_id: Uuid,
  ) -> Option<Arc<AgentMeta>> {
    let mut guard = self.inner.write();
    if guard
      .get(machine_id)
      .is_some_and(|meta| meta.connection_id == connection_id)
    {
      guard.remove(machine_id)
    } else {
      None
    }
  }

  #[must_use]
  pub fn get(&self, machine_id: &Uuid) -> Option<Arc<AgentMeta>> {
    self.inner.read().get(machine_id).map(Arc::clone)
  }

  /// Agents that advertise the given system and have a free slot. Used
  /// by the scheduler; ordering and PSI gating are applied by the caller.
  #[must_use]
  pub fn candidates_for(
    &self,
    system: &str,
  ) -> Vec<(Arc<AgentMeta>, AgentSnapshot)> {
    let candidates: Vec<Arc<AgentMeta>> = {
      let guard = self.inner.read();
      guard
        .values()
        .filter(|m| {
          m.systems.iter().any(|s| s == system)
            && m.current_jobs.load(Ordering::Relaxed) < m.max_jobs
        })
        .map(Arc::clone)
        .collect()
    };
    candidates
      .into_iter()
      .map(|m| {
        let cur = m.current_jobs.load(Ordering::Relaxed);
        let snap = snapshot(&m, cur);
        (m, snap)
      })
      .collect()
  }

  #[must_use]
  pub fn snapshot_all(&self) -> Vec<AgentSnapshot> {
    self
      .inner
      .read()
      .values()
      .map(|m| snapshot(m, m.current_jobs.load(Ordering::Relaxed)))
      .collect()
  }

  /// Free build slots across connected agents. The scheduler uses this as the
  /// per-cycle cap when fetching pending builds.
  #[must_use]
  pub fn total_free_slots(&self) -> u32 {
    self
      .inner
      .read()
      .values()
      .map(|m| {
        m.max_jobs
          .saturating_sub(m.current_jobs.load(Ordering::Relaxed))
      })
      .fold(0u32, u32::saturating_add)
  }

  /// Whether any connected agent advertises `system`, regardless of current
  /// load.
  #[must_use]
  pub fn serves_system(&self, system: &str) -> bool {
    self
      .inner
      .read()
      .values()
      .any(|m| m.systems.iter().any(|s| s == system))
  }

  /// Total advertised build slots across connected agents.
  #[must_use]
  pub fn total_slots(&self) -> u32 {
    self
      .inner
      .read()
      .values()
      .map(|m| m.max_jobs)
      .fold(0u32, u32::saturating_add)
  }

  #[must_use]
  pub fn len(&self) -> usize {
    self.inner.read().len()
  }

  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.inner.read().is_empty()
  }
}

fn snapshot(m: &AgentMeta, current_jobs: u32) -> AgentSnapshot {
  let hb = *m.heartbeat.read();
  AgentSnapshot {
    machine_id: m.machine_id,
    name: m.name.clone(),
    systems: m.systems.clone(),
    supported_features: m.supported_features.clone(),
    mandatory_features: m.mandatory_features.clone(),
    speed_factor: m.speed_factor,
    cpu_count: m.cpu_count,
    max_jobs: m.max_jobs,
    current_jobs,
    ephemeral: m.ephemeral,
    auth_kind: m.auth_kind.clone(),
    oidc_repository: m.oidc_repository.clone(),
    oidc_subject: m.oidc_subject.clone(),
    heartbeat: hb,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn meta_with(
    machine_id: Uuid,
    connection_id: Uuid,
    max_jobs: u32,
  ) -> Arc<AgentMeta> {
    let (tx, _rx) = mpsc::unbounded_channel();
    Arc::new(AgentMeta::new(
      machine_id,
      connection_id,
      format!("agent-{machine_id}"),
      "host".into(),
      vec!["x86_64-linux".into()],
      Vec::new(),
      Vec::new(),
      1.0,
      1,
      max_jobs,
      false,
      "token".into(),
      None,
      None,
      tx,
    ))
  }

  fn meta(machine_id: Uuid, connection_id: Uuid) -> Arc<AgentMeta> {
    meta_with(machine_id, connection_id, 1)
  }

  #[test]
  fn try_acquire_slot_respects_max_jobs() {
    let m = meta_with(Uuid::new_v4(), Uuid::new_v4(), 3);
    let g1 = m.try_acquire_slot();
    let g2 = m.try_acquire_slot();
    let g3 = m.try_acquire_slot();
    assert!(g1.is_some() && g2.is_some() && g3.is_some());
    assert!(m.try_acquire_slot().is_none(), "must not exceed max_jobs");
    drop(g1);
    let g4 = m.try_acquire_slot();
    assert!(g4.is_some(), "dropping a guard frees exactly one slot");
    assert!(m.try_acquire_slot().is_none());
  }

  #[test]
  fn try_acquire_slot_no_oversubscribe_under_contention() {
    let m = meta_with(Uuid::new_v4(), Uuid::new_v4(), 4);
    let succeeded = std::sync::atomic::AtomicU32::new(0);

    // Every thread holds its reservation until all have tried, so the count
    // reflects the true concurrent maximum rather than churn.
    let barrier = std::sync::Barrier::new(32);
    std::thread::scope(|s| {
      for _ in 0..32 {
        s.spawn(|| {
          let guard = m.try_acquire_slot();
          if guard.is_some() {
            succeeded.fetch_add(1, Ordering::Relaxed);
          }
          barrier.wait();
        });
      }
    });
    assert_eq!(succeeded.load(Ordering::Relaxed), 4);
    assert_eq!(m.current_jobs.load(Ordering::Relaxed), 0);
  }

  #[test]
  fn total_free_slots_sums_across_agents() {
    let pool = AgentPool::default();
    let a = meta_with(Uuid::new_v4(), Uuid::new_v4(), 4);
    let b = meta_with(Uuid::new_v4(), Uuid::new_v4(), 2);
    pool.insert(Arc::clone(&a));
    pool.insert(Arc::clone(&b));
    assert_eq!(pool.total_free_slots(), 6);
    let ga = a.try_acquire_slot();
    let gb = b.try_acquire_slot();
    assert!(ga.is_some() && gb.is_some());
    assert_eq!(pool.total_free_slots(), 4);
  }

  #[test]
  fn stale_connection_cannot_remove_replacement() {
    let pool = AgentPool::default();
    let machine_id = Uuid::new_v4();
    let old_connection = Uuid::new_v4();
    let new_connection = Uuid::new_v4();

    assert!(pool.insert(meta(machine_id, old_connection)).is_none());
    assert!(pool.insert(meta(machine_id, new_connection)).is_some());

    assert!(
      pool
        .remove_if_connection(&machine_id, old_connection)
        .is_none()
    );
    assert_eq!(
      pool.get(&machine_id).map(|m| m.connection_id),
      Some(new_connection)
    );
    assert!(
      pool
        .remove_if_connection(&machine_id, new_connection)
        .is_some()
    );
    assert!(pool.get(&machine_id).is_none());
  }
}
