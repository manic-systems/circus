use std::{fmt, str::FromStr, sync::LazyLock};

use regex::Regex;
use serde::{Deserialize, Serialize};

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
