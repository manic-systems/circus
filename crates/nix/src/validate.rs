use std::{path::Path, sync::LazyLock};

use circus_types::{InputType, validation as shared_validation};
use regex::Regex;

use crate::flake;

pub(crate) static SYSTEM_RE: LazyLock<Regex> = LazyLock::new(|| {
  #[expect(clippy::expect_used, reason = "static regex initializer, fine")]
  {
    Regex::new(r"^\w+-\w+$").expect("SYSTEM_RE failed to compile")
  }
});

/// Validate a Circus/Nix identifier.
///
/// # Errors
///
/// Returns an error if the name is empty, too long, or contains unsupported
/// characters.
pub fn validate_name(name: &str, field: &str) -> Result<(), String> {
  shared_validation::validate_name(name, field)
}

/// Validate a Git commit hash used by Nix inputs.
///
/// # Errors
///
/// Returns an error if the hash is not 1-64 hexadecimal characters.
pub fn validate_commit_hash(hash: &str) -> Result<(), String> {
  shared_validation::validate_commit_hash(hash)
}

/// Validate nix expression format.
///
/// # Errors
///
/// Returns error if expression contains invalid characters or path traversal.
pub fn validate_nix_expression(expr: &str) -> Result<(), String> {
  if expr.is_empty() {
    return Err("nix_expression cannot be empty".to_string());
  }
  if expr.len() > 1024 {
    return Err("nix_expression must be at most 1024 characters".to_string());
  }
  if expr.contains('\0') {
    return Err("nix_expression must not contain null bytes".to_string());
  }
  if expr.contains("..") {
    return Err(
      "nix_expression must not contain path traversal sequences (..)"
        .to_string(),
    );
  }
  if expr.starts_with('/') {
    return Err("nix_expression must not be an absolute path".to_string());
  }
  Ok(())
}

/// Validate a jobset input before it is persisted or passed to Nix.
///
/// # Errors
///
/// Returns error if the input is malformed or would let untrusted data point
/// Nix at the local filesystem.
pub fn validate_jobset_input(
  name: &str,
  input_type: InputType,
  value: &str,
  revision: Option<&str>,
) -> Result<(), String> {
  validate_name(name, "input name")?;
  if value.is_empty() {
    return Err("input value cannot be empty".to_string());
  }
  if value.len() > 2048 {
    return Err("input value must be at most 2048 characters".to_string());
  }
  if value.contains('\0') {
    return Err("input value must not contain null bytes".to_string());
  }
  if let Some(rev) = revision {
    validate_commit_hash(rev)?;
  }

  match input_type {
    InputType::Git => flake::Ref::parse(value).map(|_| ()),
    InputType::String | InputType::Boolean | InputType::Build => Ok(()),
  }
}

/// Validate a derivation path before persisting or passing it to Nix.
///
/// # Errors
///
/// Returns an error if the path is not absolute, is not a `.drv`, or contains
/// path traversal.
pub fn validate_drv_path(path: &str) -> Result<(), String> {
  if !path.starts_with('/') {
    return Err("drv_path must be an absolute path".to_string());
  }
  if !Path::new(path)
    .extension()
    .is_some_and(|ext| ext.eq_ignore_ascii_case("drv"))
  {
    return Err("drv_path must end with .drv".to_string());
  }
  if path.contains("..") {
    return Err("drv_path must not contain ..".to_string());
  }
  Ok(())
}

/// Validate a Nix system string.
///
/// # Errors
///
/// Returns an error if the system does not match the expected
/// architecture-platform shape.
pub fn validate_system(system: &str) -> Result<(), String> {
  if !SYSTEM_RE.is_match(system) {
    return Err("system must match pattern like x86_64-linux".to_string());
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn nix_expression_valid() {
    assert!(validate_nix_expression("packages").is_ok());
    assert!(validate_nix_expression("checks.x86_64-linux").is_ok());
    assert!(validate_nix_expression("hydraJobs").is_ok());
  }

  #[test]
  fn nix_expression_rejects_path_traversal() {
    assert!(validate_nix_expression("../../../etc/passwd").is_err());
    assert!(validate_nix_expression("packages/..").is_err());
    assert!(validate_nix_expression("a..b").is_err());
  }

  #[test]
  fn nix_expression_rejects_absolute_path() {
    assert!(validate_nix_expression("/etc/passwd").is_err());
    assert!(validate_nix_expression("/nix/store/something").is_err());
  }

  #[test]
  fn nix_expression_rejects_empty() {
    assert!(validate_nix_expression("").is_err());
  }

  #[test]
  fn nix_expression_rejects_null_bytes() {
    assert!(validate_nix_expression("packages\0evil").is_err());
  }

  #[test]
  fn jobset_input_blocks_local_path_inputs() {
    assert!(
      validate_jobset_input(
        "nixpkgs",
        InputType::Git,
        "path:/var/lib/circus",
        None
      )
      .is_err()
    );
    assert!(serde_json::from_str::<InputType>("\"path\"").is_err());
    assert!(
      validate_jobset_input(
        "nixpkgs",
        InputType::Git,
        "github:NixOS/nixpkgs",
        Some("abcdef"),
      )
      .is_ok()
    );
    assert!(
      validate_jobset_input("flag", InputType::Boolean, "true", None).is_ok()
    );
  }
}
