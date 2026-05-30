//! Circus build agent.
//!
//! A long-running daemon on each build host. Dials the queue-runner over
//! capnp-rpc, registers, and serves the `Builder` capability the runner
//! uses to push build assignments.
//!
//! Layout:
//! - [`config`]  CLI + on-disk TOML
//! - [`session`] capnp-rpc bootstrap, register, heartbeat loop
//! - [`build`]   one-shot nix build runner + log streaming
//! - [`psi`]     /proc/pressure parser
//! - [`tls`]     rustls connector builder

pub mod build;
pub mod config;
pub mod psi;
pub mod session;
pub mod tls;
pub mod upload;
