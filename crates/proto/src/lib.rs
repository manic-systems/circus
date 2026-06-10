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

/// Conservative protocol limits shared by hand-written implementations.
///
/// Cap'n Proto has traversal limits at decode time; these limits are the
/// application-level contract for fields that would otherwise be valid but
/// operationally unsafe or hard to reason about.
pub mod limits {
  pub const MAX_AGENT_NAME_LEN: usize = 128;
  pub const MAX_HOSTNAME_LEN: usize = 255;
  pub const MAX_AUTH_TOKEN_LEN: usize = 4096;
  pub const MAX_SYSTEMS: u32 = 32;
  pub const MAX_FEATURES: u32 = 128;
  pub const MAX_FEATURE_LEN: usize = 128;
  pub const MAX_STORE_PATH_LEN: usize = 4096;
  pub const MAX_HASH_LEN: usize = 512;
  pub const MAX_PRESIGNED_URL_REQUESTS: u32 = 128;
  pub const MAX_LOG_CHUNK_BYTES: usize = 1024 * 1024;
  pub const MAX_NAR_CHUNK_BYTES: usize = 4 * 1024 * 1024;
}

pub mod nix_log;

pub mod circus_capnp {
  #![allow(
    warnings,
    clippy::all,
    clippy::nursery,
    clippy::pedantic,
    clippy::restriction,
    reason = "Generated Cap'n Proto bindings are not hand-maintained Rust"
  )]

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
  output_sink,
  presigned_nar_request,
  presigned_nar_response,
  presigned_upload_opts,
  pressure_state,
  result_sink,
  runner,
  step_status,
};
