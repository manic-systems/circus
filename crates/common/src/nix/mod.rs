//! Nix integration: flake references, store paths, derivation features, and
//! flake probing.

pub mod derivation;
pub mod flake;
pub mod probe;
pub mod store;
pub mod validate;

pub use probe::{
  FlakeMetadata,
  FlakeOutput,
  FlakeProbeResult,
  SuggestedJobset,
};
pub use store::{NixHash, StorePath};
