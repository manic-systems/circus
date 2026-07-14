//! Shared domain types that must be usable below `circus-common`.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BinaryCacheUpstreams(pub Vec<BinaryCacheUpstream>);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BinaryCacheUpstream {
  pub url:        String,
  pub public_key: Option<String>,
}

pub mod validation;

#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default,
)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "clap", value(rename_all = "kebab-case"))]
pub enum GlobalRole {
  Admin,
  #[default]
  ReadOnly,
  CreateProjects,
  EvalJobset,
  CancelBuild,
  RestartJobs,
  BumpToFront,
}

impl GlobalRole {
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Admin => "admin",
      Self::ReadOnly => "read-only",
      Self::CreateProjects => "create-projects",
      Self::EvalJobset => "eval-jobset",
      Self::CancelBuild => "cancel-build",
      Self::RestartJobs => "restart-jobs",
      Self::BumpToFront => "bump-to-front",
    }
  }
}

impl fmt::Display for GlobalRole {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

impl FromStr for GlobalRole {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "admin" => Ok(Self::Admin),
      "read-only" => Ok(Self::ReadOnly),
      "create-projects" => Ok(Self::CreateProjects),
      "eval-jobset" => Ok(Self::EvalJobset),
      "cancel-build" => Ok(Self::CancelBuild),
      "restart-jobs" => Ok(Self::RestartJobs),
      "bump-to-front" => Ok(Self::BumpToFront),
      _ => Err(format!("invalid global role '{s}'")),
    }
  }
}

#[derive(
  Debug,
  Clone,
  Copy,
  PartialEq,
  Eq,
  PartialOrd,
  Ord,
  Serialize,
  Deserialize,
  Default,
)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "clap", value(rename_all = "lowercase"))]
pub enum ProjectRole {
  #[default]
  Member     = 1,
  Maintainer = 2,
  Admin      = 3,
}

impl ProjectRole {
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Member => "member",
      Self::Maintainer => "maintainer",
      Self::Admin => "admin",
    }
  }

  #[must_use]
  pub fn has_permission(self, required: Self) -> bool {
    self >= required
  }
}

impl fmt::Display for ProjectRole {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

impl FromStr for ProjectRole {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "member" => Ok(Self::Member),
      "maintainer" => Ok(Self::Maintainer),
      "admin" => Ok(Self::Admin),
      _ => Err(format!("invalid project role '{s}'")),
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuthKind {
  Token,
  Oidc,
}

impl AuthKind {
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Token => "token",
      Self::Oidc => "oidc",
    }
  }
}

impl fmt::Display for AuthKind {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

impl FromStr for AuthKind {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "token" => Ok(Self::Token),
      "oidc" => Ok(Self::Oidc),
      _ => Err(format!("invalid auth kind '{s}'")),
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ForgeType {
  Github,
  Gitea,
  Forgejo,
  Gitlab,
}

impl ForgeType {
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Github => "github",
      Self::Gitea => "gitea",
      Self::Forgejo => "forgejo",
      Self::Gitlab => "gitlab",
    }
  }
}

impl fmt::Display for ForgeType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

impl FromStr for ForgeType {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "github" => Ok(Self::Github),
      "gitea" => Ok(Self::Gitea),
      "forgejo" => Ok(Self::Forgejo),
      "gitlab" => Ok(Self::Gitlab),
      _ => Err(format!("invalid forge type '{s}'")),
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationType {
  GithubStatus,
  GiteaStatus,
  ForgejoStatus,
  GitlabStatus,
  Webhook,
  Slack,
  Email,
}

impl NotificationType {
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::GithubStatus => "github_status",
      Self::GiteaStatus => "gitea_status",
      Self::ForgejoStatus => "forgejo_status",
      Self::GitlabStatus => "gitlab_status",
      Self::Webhook => "webhook",
      Self::Slack => "slack",
      Self::Email => "email",
    }
  }

  #[must_use]
  pub const fn all() -> &'static [Self] {
    const TYPES: &[NotificationType] = &[
      NotificationType::GithubStatus,
      NotificationType::GiteaStatus,
      NotificationType::ForgejoStatus,
      NotificationType::GitlabStatus,
      NotificationType::Webhook,
      NotificationType::Slack,
      NotificationType::Email,
    ];
    TYPES
  }
}

impl fmt::Display for NotificationType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

impl FromStr for NotificationType {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "github_status" => Ok(Self::GithubStatus),
      "gitea_status" => Ok(Self::GiteaStatus),
      "forgejo_status" => Ok(Self::ForgejoStatus),
      "gitlab_status" => Ok(Self::GitlabStatus),
      "webhook" => Ok(Self::Webhook),
      "slack" => Ok(Self::Slack),
      "email" => Ok(Self::Email),
      _ => Err(format!("invalid notification type '{s}'")),
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputType {
  Git,
  String,
  Boolean,
  Build,
}

impl InputType {
  #[must_use]
  pub const fn as_str(self) -> &'static str {
    match self {
      Self::Git => "git",
      Self::String => "string",
      Self::Boolean => "boolean",
      Self::Build => "build",
    }
  }
}

impl fmt::Display for InputType {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

impl FromStr for InputType {
  type Err = String;

  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s {
      "git" => Ok(Self::Git),
      "string" => Ok(Self::String),
      "boolean" => Ok(Self::Boolean),
      "build" => Ok(Self::Build),
      _ => Err(format!("unsupported jobset input type '{s}'")),
    }
  }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "fine in tests")]
mod tests {
  use super::*;

  #[test]
  fn global_role_strings_match_serde() {
    let roles = [
      GlobalRole::Admin,
      GlobalRole::ReadOnly,
      GlobalRole::CreateProjects,
      GlobalRole::EvalJobset,
      GlobalRole::CancelBuild,
      GlobalRole::RestartJobs,
      GlobalRole::BumpToFront,
    ];

    for role in roles {
      let json = serde_json::to_string(&role).unwrap();
      assert_eq!(json, format!("\"{}\"", role.as_str()));
      assert_eq!(serde_json::from_str::<GlobalRole>(&json).unwrap(), role);
    }
  }

  #[test]
  fn project_role_ordering_matches_permissions() {
    assert!(ProjectRole::Member < ProjectRole::Maintainer);
    assert!(ProjectRole::Maintainer < ProjectRole::Admin);
    assert!(ProjectRole::Admin.has_permission(ProjectRole::Member));
    assert!(!ProjectRole::Member.has_permission(ProjectRole::Admin));
  }

  #[test]
  fn domain_enums_reject_unknown_values() {
    assert!(serde_json::from_str::<ForgeType>("\"svn\"").is_err());
    assert!(
      serde_json::from_str::<NotificationType>("\"carrier_pigeon\"").is_err()
    );
    assert!(serde_json::from_str::<InputType>("\"path\"").is_err());
  }

  #[test]
  fn auth_kind_rejects_unknown_values() {
    assert!(serde_json::from_str::<AuthKind>("\"token\"").is_ok());
    assert!(serde_json::from_str::<AuthKind>("\"oidc\"").is_ok());
    assert!(serde_json::from_str::<AuthKind>("\"password\"").is_err());
  }
}
