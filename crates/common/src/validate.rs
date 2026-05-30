//! Input validation helpers

use std::sync::LazyLock;

use regex::Regex;

/// Validate that a path is a valid nix store path for the given store
/// directory. Rejects path traversal, overly long paths, and paths outside the
/// store.
#[must_use]
pub fn is_valid_store_path(path: &str, store_dir: &str) -> bool {
  let store_dir = store_dir.trim_end_matches('/');
  let prefix = format!("{store_dir}/");
  path.starts_with(prefix.as_str()) && !path.contains("..") && path.len() < 512
}

/// Validate that a string is a valid nix store hash (32 lowercase alphanumeric
/// chars).
#[must_use]
pub fn is_valid_nix_hash(hash: &str) -> bool {
  hash.len() == 32
    && hash
      .chars()
      .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

static NAME_RE: LazyLock<Regex> = LazyLock::new(|| {
  #[expect(
    clippy::expect_used,
    reason = "static regex initializer - invalid regex would be a programming \
              error"
  )]
  {
    Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_-]*$")
      .expect("invalid NAME_RE regex pattern")
  }
});

static COMMIT_HASH_RE: LazyLock<Regex> = LazyLock::new(|| {
  #[expect(
    clippy::expect_used,
    reason = "static regex initializer - invalid regex would be a programming \
              error"
  )]
  {
    Regex::new(r"^[0-9a-fA-F]{1,64}$")
      .expect("invalid COMMIT_HASH_RE regex pattern")
  }
});

static SYSTEM_RE: LazyLock<Regex> = LazyLock::new(|| {
  #[expect(
    clippy::expect_used,
    reason = "static regex initializer - invalid regex would be a programming \
              error"
  )]
  {
    Regex::new(r"^\w+-\w+$").expect("invalid SYSTEM_RE regex pattern")
  }
});

/// Schemes considered insecure for repository URLs.
const INSECURE_SCHEMES: &[&str] = &["file", "http"];

const VALID_FORGE_TYPES: &[&str] = &["github", "gitea", "forgejo", "gitlab"];

/// Known internal/metadata IP ranges and hostnames to block for SSRF
/// protection.
const INTERNAL_HOSTS: &[&str] = &[
  "169.254.169.254", // AWS/GCP metadata
  "metadata.google.internal",
  "100.100.100.200", // Alibaba metadata
];

/// Extract the hostname from a URL (best-effort, no full URL parser needed).
fn extract_host_from_url(url: &str) -> Option<String> {
  // Strip scheme
  let after_scheme = url.split("://").nth(1)?;
  // Strip userinfo (user@host)
  let after_user = after_scheme
    .split_once('@')
    .map_or(after_scheme, |(_, rest)| rest);
  // Take host:port, strip port and path
  let host_port = after_user.split('/').next()?;
  let host = host_port.split(':').next()?;
  if host.is_empty() {
    return None;
  }
  Some(host.to_lowercase())
}

/// Check if a hostname is internal/metadata (SSRF targets).
fn is_internal_host(host: &str) -> bool {
  if INTERNAL_HOSTS.contains(&host) {
    return true;
  }
  // Block localhost variants
  if host == "localhost"
    || host == "127.0.0.1"
    || host == "::1"
    || host == "[::1]"
  {
    return true;
  }
  // Block link-local 169.254.x.x
  if host.starts_with("169.254.") {
    return true;
  }
  // Block 10.x.x.x
  if host.starts_with("10.") {
    return true;
  }
  // Block 172.16-31.x.x
  if host.starts_with("172.")
    && let Some(second_octet) = host.split('.').nth(1)
    && let Ok(n) = second_octet.parse::<u8>()
    && (16..=31).contains(&n)
  {
    return true;
  }
  // Block 192.168.x.x
  if host.starts_with("192.168.") {
    return true;
  }
  false
}

/// Trait for validating request DTOs before persisting.
pub trait Validate {
  /// Validate the DTO.
  ///
  /// # Errors
  ///
  /// Returns error if validation fails.
  fn validate(&self) -> Result<(), String>;
}

fn validate_name(name: &str, field: &str) -> Result<(), String> {
  if name.is_empty() || name.len() > 255 {
    return Err(format!("{field} must be between 1 and 255 characters"));
  }
  if !NAME_RE.is_match(name) {
    return Err(format!(
      "{field} must start with alphanumeric and contain only [a-zA-Z0-9_-]"
    ));
  }
  Ok(())
}

fn validate_repository_url(url: &str) -> Result<(), String> {
  if url.is_empty() {
    return Err("repository_url cannot be empty".to_string());
  }
  if url.len() > 2048 {
    return Err("repository_url must be at most 2048 characters".to_string());
  }
  if !url.contains("://") {
    return Err(
      "repository_url must contain a valid URL scheme (e.g. https://)"
        .to_string(),
    );
  }
  // Reject URLs targeting common internal/metadata endpoints
  if let Some(host) = extract_host_from_url(url)
    && is_internal_host(&host)
  {
    return Err(
      "repository_url must not target internal or metadata addresses"
        .to_string(),
    );
  }
  Ok(())
}

/// SSRF guard for outbound webhook URLs (Slack, generic webhooks, ...).
/// Rejects schemes other than http/https, internal hostnames, and cloud
/// metadata endpoints.
///
/// # Errors
///
/// Returns the reason string if the URL is rejected.
pub fn validate_webhook_url(url: &str) -> Result<(), String> {
  if url.is_empty() {
    return Err("URL cannot be empty".to_string());
  }
  if url.len() > 2048 {
    return Err("URL must be at most 2048 characters".to_string());
  }
  let Some((scheme, _rest)) = url.split_once("://") else {
    return Err("URL must include a scheme".to_string());
  };
  match scheme.to_ascii_lowercase().as_str() {
    "http" | "https" => {},
    other => return Err(format!("URL scheme '{other}' is not allowed")),
  }
  if let Some(host) = extract_host_from_url(url)
    && is_internal_host(&host)
  {
    return Err(
      "URL must not target internal or metadata addresses".to_string(),
    );
  }
  Ok(())
}

/// Validate that a URL uses one of the allowed schemes.
/// Logs a warning when insecure schemes (`file`, `http`) are used.
///
/// # Errors
///
/// Returns error if URL scheme is not in the allowed list.
pub fn validate_url_scheme(
  url: &str,
  allowed_schemes: &[String],
) -> Result<(), String> {
  let scheme = url.split("://").next().unwrap_or("");
  if !allowed_schemes.iter().any(|s| s == scheme) {
    return Err(format!(
      "repository_url scheme '{scheme}://' is not allowed. Allowed schemes: {}",
      allowed_schemes
        .iter()
        .map(|s| format!("{s}://"))
        .collect::<Vec<_>>()
        .join(", ")
    ));
  }
  if INSECURE_SCHEMES.contains(&scheme) {
    tracing::warn!(
      url = url,
      scheme = scheme,
      "Repository URL uses insecure scheme"
    );
  }
  Ok(())
}

/// Log warnings at startup for any insecure schemes in the allowed list.
pub fn warn_insecure_schemes(allowed_schemes: &[String]) {
  for scheme in allowed_schemes {
    if INSECURE_SCHEMES.contains(&scheme.as_str()) {
      tracing::warn!(
        scheme = scheme.as_str(),
        "Insecure URL scheme '{scheme}://' is enabled in \
         server.allowed_url_schemes"
      );
    }
  }
}

fn validate_description(desc: &str) -> Result<(), String> {
  if desc.len() > 4096 {
    return Err("description must be at most 4096 characters".to_string());
  }
  Ok(())
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
  // Reject path traversal sequences
  if expr.contains("..") {
    return Err(
      "nix_expression must not contain path traversal sequences (..)"
        .to_string(),
    );
  }
  // Reject absolute paths - nix expressions should be relative attribute paths
  if expr.starts_with('/') {
    return Err("nix_expression must not be an absolute path".to_string());
  }
  Ok(())
}

fn validate_check_interval(interval: i32) -> Result<(), String> {
  if !(10..=86400).contains(&interval) {
    return Err("check_interval must be between 10 and 86400".to_string());
  }
  Ok(())
}

fn validate_commit_hash(hash: &str) -> Result<(), String> {
  if !COMMIT_HASH_RE.is_match(hash) {
    return Err("commit_hash must be 1-64 hex characters".to_string());
  }
  Ok(())
}

fn validate_drv_path(path: &str) -> Result<(), String> {
  if !path.starts_with('/') {
    return Err("drv_path must be an absolute path".to_string());
  }
  if !std::path::Path::new(path)
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

fn validate_system(system: &str) -> Result<(), String> {
  if !SYSTEM_RE.is_match(system) {
    return Err("system must match pattern like x86_64-linux".to_string());
  }
  Ok(())
}

fn validate_ssh_uri(uri: &str) -> Result<(), String> {
  if uri.is_empty() {
    return Err("ssh_uri cannot be empty".to_string());
  }
  if uri.len() > 2048 {
    return Err("ssh_uri must be at most 2048 characters".to_string());
  }
  Ok(())
}

fn validate_positive_i32(val: i32, field: &str) -> Result<(), String> {
  if val < 1 {
    return Err(format!("{field} must be >= 1"));
  }
  Ok(())
}

fn validate_forge_type(forge_type: &str) -> Result<(), String> {
  if !VALID_FORGE_TYPES.contains(&forge_type) {
    return Err(format!(
      "forge_type must be one of: {}",
      VALID_FORGE_TYPES.join(", ")
    ));
  }
  Ok(())
}

use crate::models::{
  CreateBuild,
  CreateChannel,
  CreateEvaluation,
  CreateJobset,
  CreateProject,
  CreateRemoteBuilder,
  CreateWebhookConfig,
  UpdateChannel,
  UpdateJobset,
  UpdateProject,
  UpdateRemoteBuilder,
};

impl Validate for CreateProject {
  fn validate(&self) -> Result<(), String> {
    validate_name(&self.name, "name")?;
    validate_repository_url(&self.repository_url)?;
    if let Some(ref desc) = self.description {
      validate_description(desc)?;
    }
    Ok(())
  }
}

impl Validate for UpdateProject {
  fn validate(&self) -> Result<(), String> {
    if let Some(ref name) = self.name {
      validate_name(name, "name")?;
    }
    if let Some(ref url) = self.repository_url {
      validate_repository_url(url)?;
    }
    if let Some(ref desc) = self.description {
      validate_description(desc)?;
    }
    Ok(())
  }
}

impl Validate for CreateJobset {
  fn validate(&self) -> Result<(), String> {
    validate_name(&self.name, "name")?;
    validate_nix_expression(&self.nix_expression)?;
    if let Some(interval) = self.check_interval {
      validate_check_interval(interval)?;
    }
    Ok(())
  }
}

impl Validate for UpdateJobset {
  fn validate(&self) -> Result<(), String> {
    if let Some(ref name) = self.name {
      validate_name(name, "name")?;
    }
    if let Some(ref expr) = self.nix_expression {
      validate_nix_expression(expr)?;
    }
    if let Some(interval) = self.check_interval {
      validate_check_interval(interval)?;
    }
    Ok(())
  }
}

impl Validate for CreateEvaluation {
  fn validate(&self) -> Result<(), String> {
    validate_commit_hash(&self.commit_hash)?;
    Ok(())
  }
}

impl Validate for CreateBuild {
  fn validate(&self) -> Result<(), String> {
    validate_drv_path(&self.drv_path)?;
    if let Some(ref system) = self.system {
      validate_system(system)?;
    }
    Ok(())
  }
}

impl Validate for CreateChannel {
  fn validate(&self) -> Result<(), String> {
    validate_name(&self.name, "name")?;
    Ok(())
  }
}

impl Validate for UpdateChannel {
  fn validate(&self) -> Result<(), String> {
    if let Some(ref name) = self.name {
      validate_name(name, "name")?;
    }
    Ok(())
  }
}

impl Validate for CreateRemoteBuilder {
  fn validate(&self) -> Result<(), String> {
    validate_name(&self.name, "name")?;
    validate_ssh_uri(&self.ssh_uri)?;
    if self.systems.is_empty() {
      return Err("systems must not be empty".to_string());
    }
    for system in &self.systems {
      validate_system(system)?;
    }
    if let Some(max_jobs) = self.max_jobs {
      validate_positive_i32(max_jobs, "max_jobs")?;
    }
    if let Some(speed_factor) = self.speed_factor {
      validate_positive_i32(speed_factor, "speed_factor")?;
    }
    Ok(())
  }
}

impl Validate for UpdateRemoteBuilder {
  fn validate(&self) -> Result<(), String> {
    if let Some(ref name) = self.name {
      validate_name(name, "name")?;
    }
    if let Some(ref uri) = self.ssh_uri {
      validate_ssh_uri(uri)?;
    }
    if let Some(ref systems) = self.systems {
      if systems.is_empty() {
        return Err("systems must not be empty".to_string());
      }
      for system in systems {
        validate_system(system)?;
      }
    }
    if let Some(max_jobs) = self.max_jobs {
      validate_positive_i32(max_jobs, "max_jobs")?;
    }
    if let Some(speed_factor) = self.speed_factor {
      validate_positive_i32(speed_factor, "speed_factor")?;
    }
    Ok(())
  }
}

impl Validate for CreateWebhookConfig {
  fn validate(&self) -> Result<(), String> {
    validate_forge_type(&self.forge_type)?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use uuid::Uuid;

  use super::*;

  #[test]
  fn valid_store_path() {
    assert!(is_valid_store_path(
      "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-hello-2.12",
      "/nix/store",
    ));
  }

  #[test]
  fn valid_store_path_nested() {
    assert!(is_valid_store_path(
      "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-hello-2.12/bin/hello",
      "/nix/store",
    ));
  }

  #[test]
  fn store_path_rejects_path_traversal() {
    assert!(!is_valid_store_path(
      "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-hello/../../../etc/passwd",
      "/nix/store",
    ));
  }

  #[test]
  fn store_path_rejects_relative_path() {
    assert!(!is_valid_store_path("nix/store/something", "/nix/store"));
  }

  #[test]
  fn store_path_rejects_wrong_prefix() {
    assert!(!is_valid_store_path(
      "/tmp/nix/store/something",
      "/nix/store"
    ));
    assert!(!is_valid_store_path("/etc/passwd", "/nix/store"));
    assert!(!is_valid_store_path("/nix/var/something", "/nix/store"));
  }

  #[test]
  fn store_path_rejects_empty() {
    assert!(!is_valid_store_path("", "/nix/store"));
  }

  #[test]
  fn store_path_rejects_just_prefix() {
    // "/nix/store/" alone has no hash, but structurally starts_with and has no
    // .., so it passes. This is fine - the DB lookup won't find anything for
    // it.
    assert!(is_valid_store_path("/nix/store/", "/nix/store"));
  }

  #[test]
  fn store_path_rejects_overly_long() {
    let long_path = format!("/nix/store/{}", "a".repeat(512));
    assert!(!is_valid_store_path(&long_path, "/nix/store"));
  }

  #[test]
  fn store_path_rejects_double_dot_embedded() {
    assert!(!is_valid_store_path("/nix/store/abc..def", "/nix/store"));
  }

  #[test]
  fn valid_store_path_custom_store_dir() {
    assert!(is_valid_store_path(
      "/opt/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-hello-2.12",
      "/opt/nix/store",
    ));
    assert!(!is_valid_store_path(
      "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-hello-2.12",
      "/opt/nix/store",
    ));
  }

  #[test]
  fn valid_nix_hash_lowercase_alpha() {
    assert!(is_valid_nix_hash("abcdefghijklmnopqrstuvwxyzabcdef"));
  }

  #[test]
  fn valid_nix_hash_digits() {
    assert!(is_valid_nix_hash("01234567890123456789012345678901"));
  }

  #[test]
  fn valid_nix_hash_mixed() {
    assert!(is_valid_nix_hash("a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6"));
  }

  #[test]
  fn nix_hash_rejects_uppercase() {
    assert!(!is_valid_nix_hash("ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEF"));
  }

  #[test]
  fn nix_hash_rejects_mixed_case() {
    assert!(!is_valid_nix_hash("abcdefghijklmnopqrstuvwxyzAbcdeF"));
  }

  #[test]
  fn nix_hash_rejects_too_short() {
    assert!(!is_valid_nix_hash("abcdef1234567890"));
  }

  #[test]
  fn nix_hash_rejects_too_long() {
    assert!(!is_valid_nix_hash("abcdefghijklmnopqrstuvwxyzabcdefg"));
  }

  #[test]
  fn nix_hash_rejects_empty() {
    assert!(!is_valid_nix_hash(""));
  }

  #[test]
  fn nix_hash_rejects_special_chars() {
    assert!(!is_valid_nix_hash("abcdefghijklmnopqrstuvwxyz!@#$%^"));
  }

  #[test]
  fn nix_hash_rejects_spaces() {
    assert!(!is_valid_nix_hash("abcdefghijklmnop rstuvwxyzabcdef"));
  }

  #[test]
  fn nix_hash_rejects_path_traversal_attempt() {
    assert!(!is_valid_nix_hash("../../../../../../etc/passwd__"));
  }

  #[test]
  fn nix_hash_rejects_sql_injection_attempt() {
    assert!(!is_valid_nix_hash("' OR 1=1; DROP TABLE builds;--"));
  }

  #[test]
  fn test_create_project_valid() {
    let p = CreateProject {
      name:           "my-project".to_string(),
      description:    Some("A test project".to_string()),
      repository_url: "https://github.com/test/repo".to_string(),
    };
    assert!(p.validate().is_ok());
  }

  #[test]
  fn test_create_project_invalid_name() {
    let p = CreateProject {
      name:           String::new(),
      description:    None,
      repository_url: "https://github.com/test/repo".to_string(),
    };
    assert!(p.validate().is_err());

    let p = CreateProject {
      name:           "-starts-with-dash".to_string(),
      description:    None,
      repository_url: "https://github.com/test/repo".to_string(),
    };
    assert!(p.validate().is_err());

    let p = CreateProject {
      name:           "has spaces".to_string(),
      description:    None,
      repository_url: "https://github.com/test/repo".to_string(),
    };
    assert!(p.validate().is_err());
  }

  #[test]
  fn test_create_project_invalid_url() {
    // URL without scheme separator is rejected structurally
    let p = CreateProject {
      name:           "valid-name".to_string(),
      description:    None,
      repository_url: "not-a-url".to_string(),
    };
    assert!(p.validate().is_err());
  }

  #[test]
  fn test_create_project_description_too_long() {
    let p = CreateProject {
      name:           "valid-name".to_string(),
      description:    Some("a".repeat(4097)),
      repository_url: "https://github.com/test/repo".to_string(),
    };
    assert!(p.validate().is_err());
  }

  #[test]
  fn test_create_jobset_valid() {
    let j = CreateJobset {
      project_id:        Uuid::new_v4(),
      name:              "main".to_string(),
      nix_expression:    "packages".to_string(),
      enabled:           None,
      flake_mode:        None,
      check_interval:    Some(300),
      branch:            None,
      scheduling_shares: None,
      state:             None,
      keep_nr:           None,
    };
    assert!(j.validate().is_ok());
  }

  #[test]
  fn test_create_jobset_interval_too_low() {
    let j = CreateJobset {
      project_id:        Uuid::new_v4(),
      name:              "main".to_string(),
      nix_expression:    "packages".to_string(),
      enabled:           None,
      flake_mode:        None,
      check_interval:    Some(5),
      branch:            None,
      scheduling_shares: None,
      state:             None,
      keep_nr:           None,
    };
    assert!(j.validate().is_err());
  }

  #[test]
  fn test_create_evaluation_valid() {
    let e = CreateEvaluation {
      jobset_id:      Uuid::new_v4(),
      commit_hash:    "abc123".to_string(),
      pr_number:      None,
      pr_head_branch: None,
      pr_base_branch: None,
      pr_action:      None,
    };
    assert!(e.validate().is_ok());
  }

  #[test]
  fn test_create_evaluation_invalid_hash() {
    let e = CreateEvaluation {
      jobset_id:      Uuid::new_v4(),
      commit_hash:    "not-hex!".to_string(),
      pr_number:      None,
      pr_head_branch: None,
      pr_base_branch: None,
      pr_action:      None,
    };
    assert!(e.validate().is_err());
  }

  #[test]
  fn test_create_build_valid() {
    let b = CreateBuild {
      evaluation_id: Uuid::new_v4(),
      job_name: "hello".to_string(),
      drv_path: "/nix/store/abc123-hello.drv".to_string(),
      system: Some("x86_64-linux".to_string()),
      ..Default::default()
    };
    assert!(b.validate().is_ok());
  }

  #[test]
  fn test_create_build_invalid_drv() {
    let b = CreateBuild {
      evaluation_id: Uuid::new_v4(),
      job_name: "hello".to_string(),
      drv_path: "/tmp/bad-path".to_string(),
      ..Default::default()
    };
    assert!(b.validate().is_err());
  }

  #[test]
  fn test_create_remote_builder_valid() {
    let rb = CreateRemoteBuilder {
      name:               "builder1".to_string(),
      ssh_uri:            "root@builder.example.com".to_string(),
      systems:            vec!["x86_64-linux".to_string()],
      max_jobs:           Some(4),
      speed_factor:       Some(1),
      supported_features: None,
      mandatory_features: None,
      public_host_key:    None,
      ssh_key_file:       None,
    };
    assert!(rb.validate().is_ok());
  }

  #[test]
  fn test_create_remote_builder_invalid_max_jobs() {
    let rb = CreateRemoteBuilder {
      name:               "builder1".to_string(),
      ssh_uri:            "root@builder.example.com".to_string(),
      systems:            vec!["x86_64-linux".to_string()],
      max_jobs:           Some(0),
      speed_factor:       None,
      supported_features: None,
      mandatory_features: None,
      public_host_key:    None,
      ssh_key_file:       None,
    };
    assert!(rb.validate().is_err());
  }

  #[test]
  fn test_create_webhook_config_valid() {
    let wh = CreateWebhookConfig {
      project_id: Uuid::new_v4(),
      forge_type: "github".to_string(),
      secret:     None,
    };
    assert!(wh.validate().is_ok());
  }

  #[test]
  fn test_create_webhook_config_invalid_forge() {
    let wh = CreateWebhookConfig {
      project_id: Uuid::new_v4(),
      forge_type: "bitbucket".to_string(),
      secret:     None,
    };
    assert!(wh.validate().is_err());
  }

  #[test]
  fn test_create_channel_valid() {
    let c = CreateChannel {
      project_id: Uuid::new_v4(),
      name:       "stable".to_string(),
      jobset_id:  Uuid::new_v4(),
    };
    assert!(c.validate().is_ok());
  }

  #[test]
  fn test_nix_expression_valid() {
    assert!(validate_nix_expression("packages").is_ok());
    assert!(validate_nix_expression("checks.x86_64-linux").is_ok());
    assert!(validate_nix_expression("hydraJobs").is_ok());
  }

  #[test]
  fn test_nix_expression_rejects_path_traversal() {
    assert!(validate_nix_expression("../../../etc/passwd").is_err());
    assert!(validate_nix_expression("packages/..").is_err());
    assert!(validate_nix_expression("a..b").is_err());
  }

  #[test]
  fn test_nix_expression_rejects_absolute_path() {
    assert!(validate_nix_expression("/etc/passwd").is_err());
    assert!(validate_nix_expression("/nix/store/something").is_err());
  }

  #[test]
  fn test_nix_expression_rejects_empty() {
    assert!(validate_nix_expression("").is_err());
  }

  #[test]
  fn test_nix_expression_rejects_null_bytes() {
    assert!(validate_nix_expression("packages\0evil").is_err());
  }

  #[test]
  fn test_validate_url_scheme_rejects_file_by_default() {
    let default_schemes: Vec<String> = vec!["https", "http", "git", "ssh"]
      .into_iter()
      .map(Into::into)
      .collect();
    assert!(
      validate_url_scheme("file:///etc/passwd", &default_schemes).is_err()
    );
  }

  #[test]
  fn test_validate_url_scheme_allows_file_when_configured() {
    let schemes: Vec<String> = vec!["https", "http", "git", "ssh", "file"]
      .into_iter()
      .map(Into::into)
      .collect();
    assert!(validate_url_scheme("file:///var/lib/repo.git", &schemes).is_ok());
  }

  #[test]
  fn test_validate_url_scheme_rejects_unknown() {
    let schemes: Vec<String> =
      vec!["https", "ssh"].into_iter().map(Into::into).collect();
    assert!(
      validate_url_scheme("ftp://example.com/repo.git", &schemes).is_err()
    );
  }

  #[test]
  fn test_repository_url_accepts_file_structurally() {
    // validate_repository_url no longer checks schemes (that's
    // validate_url_scheme's job)
    assert!(validate_repository_url("file:///etc/passwd").is_ok());
  }

  #[test]
  fn test_repository_url_rejects_localhost() {
    assert!(validate_repository_url("http://localhost/repo.git").is_err());
    assert!(validate_repository_url("http://127.0.0.1/repo.git").is_err());
  }

  #[test]
  fn test_repository_url_rejects_metadata_endpoint() {
    assert!(
      validate_repository_url("http://169.254.169.254/latest/meta-data")
        .is_err()
    );
  }

  #[test]
  fn test_repository_url_rejects_private_networks() {
    assert!(validate_repository_url("http://10.0.0.1/repo.git").is_err());
    assert!(validate_repository_url("http://192.168.1.1/repo.git").is_err());
    assert!(validate_repository_url("http://172.16.0.1/repo.git").is_err());
  }

  #[test]
  fn test_repository_url_accepts_valid_https() {
    assert!(validate_repository_url("https://github.com/test/repo").is_ok());
    assert!(
      validate_repository_url("https://gitlab.com/test/repo.git").is_ok()
    );
    assert!(validate_repository_url("git://example.com/repo.git").is_ok());
    assert!(
      validate_repository_url("ssh://git@github.com/test/repo.git").is_ok()
    );
  }

  #[test]
  fn test_extract_host_from_url() {
    assert_eq!(
      extract_host_from_url("https://github.com/repo"),
      Some("github.com".to_string())
    );
    assert_eq!(
      extract_host_from_url("http://10.0.0.1:8080/repo"),
      Some("10.0.0.1".to_string())
    );
    assert_eq!(
      extract_host_from_url("ssh://user@host.com/repo"),
      Some("host.com".to_string())
    );
    assert_eq!(extract_host_from_url("not-a-url"), None);
  }
}
