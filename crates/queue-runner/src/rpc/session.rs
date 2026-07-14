//! `AgentSession` implementation hosted by the runner.
//!
//! This is the capability the agent holds for outbound traffic. Today
//! that's just `heartbeat`; future additions (e.g. `requestWork` for
//! pull-based scheduling, `noteSubstitute` for substitution metrics) plug
//! in here.

use std::sync::Arc;

use capnp::capability::Rc;
use circus_common::{PgPool, repo};
use circus_proto::agent_session;
use uuid::Uuid;

use super::pool::{AgentPool, HeartbeatSnapshot};

pub struct SessionImpl {
  pub machine_id: Uuid,
  pub pool:       Arc<AgentPool>,
  pub db_pool:    PgPool,
}

#[allow(refining_impl_trait_internal, refining_impl_trait_reachable)]
impl agent_session::Server for SessionImpl {
  async fn heartbeat(
    self: Rc<Self>,
    params: agent_session::HeartbeatParams,
    _results: agent_session::HeartbeatResults,
  ) -> Result<(), capnp::Error> {
    #![expect(
      clippy::future_not_send,
      reason = "capnp-rpc session capability is !Send; the RPC thread uses a \
                single-threaded runtime"
    )]
    let pr = params.get()?;
    let ping = pr.get_ping()?;
    let pressure = ping.get_pressure()?;

    let load1 = ping.get_load1();
    let load5 = ping.get_load5();
    let load15 = ping.get_load15();
    let cpu_psi = pressure.get_cpu_avg10();
    let mem_psi = pressure.get_mem_avg10();
    let io_psi = pressure.get_io_avg10();
    let current_jobs = ping.get_current_jobs();
    let mem_total = ping.get_mem_total();
    let mem_used = ping.get_mem_used();
    let store_free = ping.get_store_free();
    let build_dir_free = ping.get_build_dir_free();

    let snap = HeartbeatSnapshot {
      last_seen: Some(std::time::Instant::now()),
      load1,
      load5,
      load15,
      cpu_psi_avg10: cpu_psi,
      mem_psi_avg10: mem_psi,
      io_psi_avg10: io_psi,
    };

    if let Some(h) = self.pool.get(&self.machine_id) {
      *h.heartbeat.write() = snap;
    } else {
      tracing::debug!(
        machine_id = %self.machine_id,
        "heartbeat for unknown agent; ignoring"
      );
    }

    let machine_id = self.machine_id;
    let db = self.db_pool.clone();
    let current_jobs = i32::try_from(current_jobs).unwrap_or(i32::MAX);
    let mem_total = i64::try_from(mem_total).unwrap_or(i64::MAX);
    let mem_used = i64::try_from(mem_used).unwrap_or(i64::MAX);
    let store_free = i64::try_from(store_free).unwrap_or(i64::MAX);
    let build_dir_free = i64::try_from(build_dir_free).unwrap_or(i64::MAX);
    let flush = repo::builder_sessions::heartbeat(
      &db,
      repo::builder_sessions::Heartbeat {
        machine_id,
        load1,
        load5,
        load15,
        cpu_psi_avg10: cpu_psi,
        mem_psi_avg10: mem_psi,
        io_psi_avg10: io_psi,
        current_jobs,
        mem_total,
        mem_used,
        store_free,
        build_dir_free,
      },
    );
    if let Err(e) = flush.await {
      tracing::warn!(%machine_id, "heartbeat db flush: {e}");
    }

    Ok(())
  }
}
