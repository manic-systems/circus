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
  pub presigned_upload: Option<String>,
  /// Completion signal: the per-connection task sends the outcome here
  /// after the agent reports via `ResultSink`. Some scheduler errors are
  /// also surfaced here (queue full, connection closed mid-dispatch).
  pub completion:       tokio::sync::oneshot::Sender<DispatchResult>,
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

/// Metadata side of an agent. Held in `AgentPool` and shared with the
/// scheduler. Send + Sync.
pub struct AgentMeta {
  pub machine_id:         Uuid,
  pub name:               String,
  pub hostname:           String,
  pub systems:            Vec<String>,
  pub supported_features: Vec<String>,
  pub mandatory_features: Vec<String>,
  pub speed_factor:       f32,
  pub cpu_count:          u32,
  pub max_jobs:           u32,

  pub current_jobs:  Arc<AtomicU32>,
  pub active_builds: RwLock<HashSet<Uuid>>,

  pub heartbeat:     RwLock<HeartbeatSnapshot>,
  pub registered_at: Instant,

  /// Hand-off into the connection task.
  pub tx: mpsc::UnboundedSender<DispatchCommand>,
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

  pub fn insert(&self, meta: Arc<AgentMeta>) {
    self.inner.write().insert(meta.machine_id, meta);
  }

  pub fn remove(&self, machine_id: &Uuid) -> Option<Arc<AgentMeta>> {
    self.inner.write().remove(machine_id)
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
    heartbeat: hb,
  }
}
