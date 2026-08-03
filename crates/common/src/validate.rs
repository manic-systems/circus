//! Input validation helpers

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use circus_types::validation as shared_validation;

/// Schemes considered insecure for repository URLs.
const INSECURE_SCHEMES: &[&str] = &["file", "http"];

/// Known internal/metadata IP ranges and hostnames to block for SSRF
/// protection.
const INTERNAL_HOSTS: &[&str] = &[
  "169.254.169.254", // AWS/GCP metadata
  "metadata.google.internal",
  "100.100.100.200", // Alibaba metadata
];

fn extract_host_from_url(url: &str) -> Option<String> {
  url::Url::parse(url)
    .ok()
    .and_then(|u| u.host_str().map(str::to_lowercase))
}

fn is_internal_host(host: &str) -> bool {
  let host = host.trim_matches(['[', ']']);
  if INTERNAL_HOSTS.contains(&host)
    || host == "localhost"
    || host.ends_with(".internal")
  {
    return true;
  }

  let Ok(ip) = host.parse::<IpAddr>() else {
    return false;
  };

  match ip {
    IpAddr::V4(v4) => is_internal_ipv4(v4),
    IpAddr::V6(v6) => {
      v6.is_loopback()
        || is_unique_local_ipv6(v6)
        || is_link_local_ipv6(v6)
        || v6.to_ipv4_mapped().is_some_and(is_internal_ipv4)
    },
  }
}

const fn is_internal_ipv4(ip: Ipv4Addr) -> bool {
  ip.is_private() || ip.is_loopback() || ip.is_link_local()
}

const fn is_unique_local_ipv6(ip: Ipv6Addr) -> bool {
  (ip.segments()[0] & 0xFE00) == 0xFC00
}

const fn is_link_local_ipv6(ip: Ipv6Addr) -> bool {
  (ip.segments()[0] & 0xFFC0) == 0xFE80
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

pub(crate) fn validate_name(name: &str, field: &str) -> Result<(), String> {
  shared_validation::validate_name(name, field)
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

/// SSRF guard for outbound notification webhook URLs that must be encrypted in
/// transit (generic webhooks, Slack). Stricter than [`validate_webhook_url`]:
/// the scheme must be `https` so secrets and HMAC signatures are not sent in
/// cleartext.
///
/// # Errors
///
/// Returns the reason string if the URL is rejected.
pub fn validate_https_webhook_url(url: &str) -> Result<(), String> {
  validate_webhook_url(url)?;
  let scheme = url.split("://").next().unwrap_or("");
  if !scheme.eq_ignore_ascii_case("https") {
    return Err("URL must use https:// for notification delivery".to_string());
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

fn validate_check_interval(interval: i32) -> Result<(), String> {
  if !(10..=86400).contains(&interval) {
    return Err("check_interval must be between 10 and 86400".to_string());
  }
  Ok(())
}

fn validate_systems(systems: &[String]) -> Result<(), String> {
  if systems.iter().any(|system| system.trim().is_empty()) {
    return Err("systems entries cannot be empty".to_string());
  }
  Ok(())
}

pub(crate) fn validate_commit_hash(hash: &str) -> Result<(), String> {
  shared_validation::validate_commit_hash(hash)
}

fn validate_ssh_uri(uri: &str) -> Result<(), String> {
  if uri.is_empty() {
    return Err("ssh_uri cannot be empty".to_string());
  }
  if uri.len() > 2048 {
    return Err("ssh_uri must be at most 2048 characters".to_string());
  }
  if uri.contains('\0') {
    return Err("ssh_uri must not contain null bytes".to_string());
  }
  // A store URI is a single token; whitespace means it is malformed.
  if uri.chars().any(char::is_whitespace) {
    return Err("ssh_uri must not contain whitespace".to_string());
  }
  // If a scheme is present it must be one nix understands for SSH stores.
  if let Some((scheme, _rest)) = uri.split_once("://")
    && scheme != "ssh"
    && scheme != "ssh-ng"
  {
    return Err(format!(
      "ssh_uri scheme '{scheme}://' is not allowed (use ssh:// or ssh-ng://)"
    ));
  }
  // Must resolve to a non-empty host, optionally behind `user@`.
  let after_scheme = uri.split_once("://").map_or(uri, |(_, rest)| rest);
  let host_part = after_scheme
    .split_once('@')
    .map_or(after_scheme, |(_, rest)| rest);
  let host = host_part.split([':', '/']).next().unwrap_or("");
  if host.is_empty() {
    return Err("ssh_uri must include a host".to_string());
  }
  Ok(())
}

/// Validate a recorded SSH host key used for connection pinning. Accepts a
/// bare key (`<type> <base64>`) or a full `known_hosts` line
/// (`<host> <type> <base64>`), optionally with a trailing comment.
fn validate_ssh_host_key(key: &str) -> Result<(), String> {
  let key = key.trim();
  if key.is_empty() {
    return Err("public_host_key cannot be empty".to_string());
  }
  if key.len() > 4096 {
    return Err("public_host_key must be at most 4096 characters".to_string());
  }
  if key.contains('\0') {
    return Err("public_host_key must not contain null bytes".to_string());
  }
  if key.contains('\n') || key.contains('\r') {
    return Err("public_host_key must be a single line".to_string());
  }
  // Find the key-type token and require a base64 blob to follow it.
  let tokens: Vec<&str> = key.split_whitespace().collect();
  let type_idx = tokens.iter().position(|t| {
    t.starts_with("ssh-") || t.starts_with("ecdsa-") || t.starts_with("sk-")
  });
  let Some(type_idx) = type_idx else {
    return Err(
      "public_host_key must contain a key type (ssh-ed25519, ssh-rsa, \
       ecdsa-..., sk-...)"
        .to_string(),
    );
  };
  // The key type must sit at index 0 or 1. A leading comment makes ssh
  // ignore the whole line.
  if type_idx > 1 {
    return Err(
      "public_host_key has unexpected tokens before the key type".to_string(),
    );
  }
  if type_idx == 1 && tokens[0].starts_with('#') {
    return Err(
      "public_host_key host pattern must not be a comment".to_string(),
    );
  }
  let Some(blob) = tokens.get(type_idx + 1) else {
    return Err(
      "public_host_key is missing its base64 key material".to_string(),
    );
  };
  let is_base64 = !blob.is_empty()
    && blob
      .chars()
      .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=');
  if !is_base64 {
    return Err(
      "public_host_key base64 material contains invalid characters".to_string(),
    );
  }
  Ok(())
}

/// Validate an SSH private key file path. Existence isn't checked (the file
/// lives on the queue-runner host); this only rejects malformed paths.
fn validate_ssh_key_file(path: &str) -> Result<(), String> {
  if path.is_empty() {
    return Err("ssh_key_file cannot be empty".to_string());
  }
  if path.len() > 4096 {
    return Err("ssh_key_file must be at most 4096 characters".to_string());
  }
  if path.contains('\0') {
    return Err("ssh_key_file must not contain null bytes".to_string());
  }
  if !path.starts_with('/') {
    return Err("ssh_key_file must be an absolute path".to_string());
  }
  if path.contains("..") {
    return Err("ssh_key_file must not contain '..'".to_string());
  }
  Ok(())
}

fn validate_positive_i32(val: i32, field: &str) -> Result<(), String> {
  if val < 1 {
    return Err(format!("{field} must be >= 1"));
  }
  Ok(())
}

use circus_types::validation::{
  validate_binary_cache_upstream,
  validate_cache_url,
};

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
    if let Some(url) = &self.cache_url {
      validate_cache_url(url, "cache_url")?;
    }
    for upstream in &self.cache_upstreams.0 {
      validate_binary_cache_upstream(upstream, "cache_upstreams")?;
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
    if let Some(url) = &self.cache_url {
      validate_cache_url(url, "cache_url")?;
    }
    if let Some(upstreams) = &self.cache_upstreams {
      for upstream in &upstreams.0 {
        validate_binary_cache_upstream(upstream, "cache_upstreams")?;
      }
    }
    Ok(())
  }
}

impl Validate for CreateJobset {
  fn validate(&self) -> Result<(), String> {
    validate_name(&self.name, "name")?;
    circus_nix::validate::validate_nix_expression(&self.nix_expression)?;
    if let Some(interval) = self.check_interval {
      validate_check_interval(interval)?;
    }
    if let Some(systems) = &self.systems {
      if systems.is_empty() {
        return Err("systems cannot be empty".to_string());
      }
      validate_systems(systems)?;
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
      circus_nix::validate::validate_nix_expression(expr)?;
    }
    if let Some(interval) = self.check_interval {
      validate_check_interval(interval)?;
    }
    if let Some(systems) = &self.systems {
      validate_systems(systems)?;
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
    circus_nix::validate::validate_drv_path(&self.drv_path)?;
    if let Some(ref system) = self.system {
      circus_nix::validate::validate_system(system)?;
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
      circus_nix::validate::validate_system(system)?;
    }
    if let Some(max_jobs) = self.max_jobs {
      validate_positive_i32(max_jobs, "max_jobs")?;
    }
    if let Some(speed_factor) = self.speed_factor {
      validate_positive_i32(speed_factor, "speed_factor")?;
    }
    if let Some(ref host_key) = self.public_host_key {
      validate_ssh_host_key(host_key)?;
    }
    if let Some(ref key_file) = self.ssh_key_file {
      validate_ssh_key_file(key_file)?;
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
        circus_nix::validate::validate_system(system)?;
      }
    }
    if let Some(max_jobs) = self.max_jobs {
      validate_positive_i32(max_jobs, "max_jobs")?;
    }
    if let Some(speed_factor) = self.speed_factor {
      validate_positive_i32(speed_factor, "speed_factor")?;
    }
    if let Some(ref host_key) = self.public_host_key {
      validate_ssh_host_key(host_key)?;
    }
    if let Some(ref key_file) = self.ssh_key_file {
      validate_ssh_key_file(key_file)?;
    }
    Ok(())
  }
}

impl Validate for CreateWebhookConfig {
  fn validate(&self) -> Result<(), String> {
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use uuid::Uuid;

  use super::*;
  use crate::models::BinaryCacheUpstreams;

  #[test]
  fn test_create_project_valid() {
    let p = CreateProject {
      name:            "my-project".to_string(),
      description:     Some("A test project".to_string()),
      repository_url:  "https://github.com/test/repo".to_string(),
      cache_enabled:   true,
      cache_url:       None,
      cache_upstreams: BinaryCacheUpstreams::default(),
    };
    assert!(p.validate().is_ok());
  }

  #[test]
  fn test_create_project_invalid_name() {
    let p = CreateProject {
      name:            String::new(),
      description:     None,
      repository_url:  "https://github.com/test/repo".to_string(),
      cache_enabled:   true,
      cache_url:       None,
      cache_upstreams: BinaryCacheUpstreams::default(),
    };
    assert!(p.validate().is_err());

    let p = CreateProject {
      name:            "-starts-with-dash".to_string(),
      description:     None,
      repository_url:  "https://github.com/test/repo".to_string(),
      cache_enabled:   true,
      cache_url:       None,
      cache_upstreams: BinaryCacheUpstreams::default(),
    };
    assert!(p.validate().is_err());

    let p = CreateProject {
      name:            "has spaces".to_string(),
      description:     None,
      repository_url:  "https://github.com/test/repo".to_string(),
      cache_enabled:   true,
      cache_url:       None,
      cache_upstreams: BinaryCacheUpstreams::default(),
    };
    assert!(p.validate().is_err());
  }

  #[test]
  fn test_create_project_invalid_url() {
    // URL without scheme separator is rejected structurally
    let p = CreateProject {
      name:            "valid-name".to_string(),
      description:     None,
      repository_url:  "not-a-url".to_string(),
      cache_enabled:   true,
      cache_url:       None,
      cache_upstreams: BinaryCacheUpstreams::default(),
    };
    assert!(p.validate().is_err());
  }

  #[test]
  fn test_create_project_description_too_long() {
    let p = CreateProject {
      name:            "valid-name".to_string(),
      description:     Some("a".repeat(4097)),
      repository_url:  "https://github.com/test/repo".to_string(),
      cache_enabled:   true,
      cache_url:       None,
      cache_upstreams: BinaryCacheUpstreams::default(),
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
      trigger_mode:      None,
      branch:            None,
      branch_pattern:    None,
      tag_pattern:       None,
      scheduling_shares: None,
      state:             None,
      keep_nr:           None,
      systems:           None,
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
      trigger_mode:      None,
      branch:            None,
      branch_pattern:    None,
      tag_pattern:       None,
      scheduling_shares: None,
      state:             None,
      keep_nr:           None,
      systems:           None,
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
  fn ssh_uri_accepts_common_forms() {
    assert!(validate_ssh_uri("root@builder.example.com").is_ok());
    assert!(validate_ssh_uri("ssh://root@builder.example.com:2222").is_ok());
    assert!(validate_ssh_uri("ssh-ng://builder.example.com").is_ok());
    assert!(validate_ssh_uri("builder.example.com").is_ok());
  }

  #[test]
  fn ssh_uri_rejects_malformed() {
    assert!(validate_ssh_uri("").is_err());
    assert!(validate_ssh_uri("root@host with space").is_err());
    assert!(validate_ssh_uri("http://host").is_err());
    assert!(validate_ssh_uri("ssh://").is_err());
    assert!(validate_ssh_uri("root@host\0").is_err());
  }

  #[test]
  fn ssh_host_key_accepts_bare_and_full_lines() {
    assert!(
      validate_ssh_host_key("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIabc").is_ok()
    );
    assert!(
      validate_ssh_host_key(
        "builder.example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIabc"
      )
      .is_ok()
    );
    assert!(
      validate_ssh_host_key("ecdsa-sha2-nistp256 AAAAE2VjZHNhLXc=").is_ok()
    );
  }

  #[test]
  fn ssh_host_key_rejects_garbage() {
    assert!(validate_ssh_host_key("").is_err());
    assert!(validate_ssh_host_key("not-a-key").is_err());
    assert!(validate_ssh_host_key("ssh-ed25519").is_err());
    assert!(validate_ssh_host_key("ssh-ed25519 bad!@#blob").is_err());
    assert!(validate_ssh_host_key("ssh-ed25519 AAAA\nsecond line").is_err());
  }

  #[test]
  fn ssh_key_file_requires_clean_absolute_path() {
    assert!(validate_ssh_key_file("/var/lib/circus/id_ed25519").is_ok());
    assert!(validate_ssh_key_file("relative/path").is_err());
    assert!(validate_ssh_key_file("/etc/../root/.ssh/id").is_err());
    assert!(validate_ssh_key_file("").is_err());
    assert!(validate_ssh_key_file("/path\0null").is_err());
  }

  #[test]
  fn remote_builder_validates_host_key_and_key_file() {
    let rb = CreateRemoteBuilder {
      name:               "builder1".to_string(),
      ssh_uri:            "root@builder.example.com".to_string(),
      systems:            vec!["x86_64-linux".to_string()],
      max_jobs:           Some(4),
      speed_factor:       Some(1),
      supported_features: None,
      mandatory_features: None,
      public_host_key:    Some("not a valid key".to_string()),
      ssh_key_file:       None,
    };
    assert!(rb.validate().is_err());

    let rb = CreateRemoteBuilder {
      public_host_key: Some(
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIabc".to_string(),
      ),
      ssh_key_file: Some("relative".to_string()),
      ..rb
    };
    assert!(rb.validate().is_err());
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
      forge_type: crate::models::ForgeType::Github,
      secret:     None,
    };
    assert!(wh.validate().is_ok());
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
  fn test_repository_url_rejects_internal_ipv6_networks() {
    assert!(validate_repository_url("http://[::1]/repo.git").is_err());
    assert!(validate_repository_url("http://[fc00::1]/repo.git").is_err());
    assert!(validate_repository_url("http://[fe80::1]/repo.git").is_err());
    assert!(
      validate_repository_url("http://[::ffff:192.168.1.1]/repo.git").is_err()
    );
  }

  #[test]
  fn test_repository_url_rejects_internal_hostnames() {
    assert!(
      validate_repository_url("http://service.internal/repo.git").is_err()
    );
    assert!(
      validate_repository_url("http://metadata.google.internal/repo.git")
        .is_err()
    );
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
