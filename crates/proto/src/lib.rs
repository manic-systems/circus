//! Generated Cap'n Proto bindings for the Circus runner <-> agent protocol.
//!
//! The schema lives at `schema/circus.capnp`. `build.rs` runs `capnpc` and
//! drops the generated Rust into `$OUT_DIR/circus_capnp.rs`. Capnp emits
//! internal references like `crate::circus_capnp::output_info::Owned`, so
//! the include MUST sit inside a module named `circus_capnp` at the crate
//! root. The convenient names are re-exported below.
//!
//! See `docs/DISTRIBUTED.md` for the protocol overview. It might be outdated
//! at any given time, but it'll give you a decent enough idea.

/// Wire-format version. Increment on any breaking schema change. The
/// agent and runner exchange this on `register` and refuse to talk on
/// mismatch.
pub const PROTO_VERSION: &str = "circus-proto/2";

pub mod circus_capnp {
  include!(concat!(env!("OUT_DIR"), "/circus_capnp.rs"));
}

pub use circus_capnp::{
  BuildOutcome,
  StepStatus,
  agent_info,
  agent_session,
  build_assignment,
  build_outcome,
  build_result,
  builder,
  heartbeat,
  log_sink,
  nar_info,
  output_info,
  presigned_nar_request,
  presigned_nar_response,
  presigned_upload_opts,
  pressure_state,
  result_sink,
  runner,
  step_status,
};
