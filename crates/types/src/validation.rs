use std::{fmt, str::FromStr, sync::LazyLock};

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::BinaryCacheUpstream;

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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Name(String);

impl Name {
  /// Build a validated Circus identifier.
  ///
  /// # Errors
  ///
  /// Returns an error if the value is empty, too long, or contains characters
  /// outside the shared identifier grammar.
  pub fn new(value: impl Into<String>) -> Result<Self, String> {
    let value = value.into();
    validate_name(&value, "name")?;
    Ok(Self(value))
  }

  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }

  #[must_use]
  pub fn into_inner(self) -> String {
    self.0
  }
}

impl fmt::Display for Name {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

impl FromStr for Name {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    Self::new(s)
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CommitHash(String);

impl CommitHash {
  /// Build a validated Git commit hash.
  ///
  /// # Errors
  ///
  /// Returns an error if the value is not a 1-64 character hexadecimal hash.
  pub fn new(value: impl Into<String>) -> Result<Self, String> {
    let value = value.into();
    validate_commit_hash(&value)?;
    Ok(Self(value))
  }

  #[must_use]
  pub fn as_str(&self) -> &str {
    &self.0
  }

  #[must_use]
  pub fn into_inner(self) -> String {
    self.0
  }
}

impl fmt::Display for CommitHash {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

impl FromStr for CommitHash {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    Self::new(s)
  }
}

/// Validate a shared Circus identifier.
///
/// # Errors
///
/// Returns an error if the value is empty, too long, or contains characters
/// outside the shared identifier grammar.
pub fn validate_name(name: &str, field: &str) -> Result<(), String> {
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

/// Validate a Git commit hash.
///
/// # Errors
///
/// Returns an error if the value is not a 1-64 character hexadecimal hash.
pub fn validate_commit_hash(hash: &str) -> Result<(), String> {
  if !COMMIT_HASH_RE.is_match(hash) {
    return Err("commit_hash must be 1-64 hex characters".to_string());
  }
  Ok(())
}

/// Validate a binary cache URL accepted by global and project cache settings.
///
/// # Errors
///
/// Returns an error if the URL is empty, too long, or uses an unsupported
/// scheme.
pub fn validate_cache_url(url: &str, field: &str) -> Result<(), String> {
  if url.trim().is_empty() {
    return Err(format!("{field} cannot be empty"));
  }
  if url.len() > 2048 {
    return Err(format!("{field} must be at most 2048 characters"));
  }
  if !matches!(
    url.split_once("://").map(|(scheme, _)| scheme),
    Some("http" | "https" | "s3" | "ssh" | "ssh-ng" | "file")
  ) {
    return Err(format!(
      "{field} must use http, https, s3, ssh, ssh-ng, or file"
    ));
  }
  Ok(())
}

/// Validate one binary cache upstream entry.
///
/// # Errors
///
/// Returns an error if the upstream URL or public key field is invalid.
pub fn validate_binary_cache_upstream(
  upstream: &BinaryCacheUpstream,
  field: &str,
) -> Result<(), String> {
  validate_cache_url(&upstream.url, &format!("{field}.url"))?;
  if upstream
    .public_key
    .as_deref()
    .is_some_and(|key| key.trim().is_empty())
  {
    return Err(format!("{field}.public_key cannot be empty"));
  }
  Ok(())
}
