//! Nix integration: flake references, store paths, derivation features, and
//! flake probing.

pub mod base32;
pub mod derivation;
pub mod error;
pub mod flake;
pub mod probe;
pub mod store;
pub mod validate;

pub use error::{Error, Result};
pub use probe::{
  FlakeMetadata,
  FlakeOutput,
  FlakeProbeResult,
  SuggestedJobset,
};
pub use store::{NixHash, StorePath};
