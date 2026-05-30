//! `AgentSession` implementation hosted by the runner.
//!
//! This is the capability the agent holds for outbound traffic. Today
//! that's just `heartbeat`; future additions (e.g. `requestWork` for
//! pull-based scheduling, `noteSubstitute` for substitution metrics) plug
//! in here.

use std::sync::Arc;

use capnp::capability::Rc;
use circus_proto::agent_session;
use sqlx::PgPool;
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
    let pr = params.get()?;
    let ping = pr.get_ping()?;
    let pressure = ping.get_pressure()?;

    let load1 = ping.get_load1();
    let load5 = ping.get_load5();
    let load15 = ping.get_load15();
    let cpu_psi = pressure.get_cpu_avg10();
    let mem_psi = pressure.get_mem_avg10();
    let io_psi = pressure.get_io_avg10();

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
    if let Err(e) = sqlx::query(
      "UPDATE builder_sessions SET last_seen = NOW(), load1 = $2, load5 = $3, \
       load15 = $4, cpu_psi_avg10 = $5, mem_psi_avg10 = $6, io_psi_avg10 = \
       $7, updated_at = NOW() WHERE machine_id = $1",
    )
    .bind(machine_id)
    .bind(load1)
    .bind(load5)
    .bind(load15)
    .bind(cpu_psi)
    .bind(mem_psi)
    .bind(io_psi)
    .execute(&db)
    .await
    {
      tracing::warn!(%machine_id, "heartbeat db flush: {e}");
    }

    Ok(())
  }
}
