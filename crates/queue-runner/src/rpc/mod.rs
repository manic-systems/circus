//! Cap'n Proto RPC server, agent pool, and dispatch glue.
//!
//! The runner holds a process-global [`AgentPool`]. When an agent connects,
//! the bootstrap `Runner` capability accepts `register`, validates the
//! bearer token, persists the agent row in `builder_sessions`, and inserts
//! a live [`pool::AgentMeta`] into the pool. The pool is read by the
//! scheduler in `worker.rs` to pick a target before falling back to SSH.
//!
//! See `docs/DISTRIBUTED.md` for the protocol overview.

pub mod log_sink;
pub mod pool;
pub mod result_sink;
pub mod s3;
pub mod server;
pub mod session;
pub mod tls;

pub use pool::{AgentHandle, AgentPool, AgentSnapshot};
pub use server::serve;
