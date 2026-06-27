//! The delivery-facing snapshot of a build event.
//!
//! [`BuildEvent`] is built once from a [`Build`] and [`Project`] and carries
//! everything a notification channel needs to render and deliver a message. It
//! is also the unit serialized into the retry-queue task payload, so it owns
//! the status-to-forge mappings that delivery relies on.

use circus_common::models::{Build, BuildStatus, Project};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A point-in-time view of a build, decoupled from the live database rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildEvent {
  pub build_id:     Uuid,
  pub status:       BuildStatus,
  pub job_name:     String,
  pub drv_path:     String,
  pub build_output: Option<String>,
  pub project_name: String,
  /// The project's repository URL (used to derive forge owner/repo/project).
  pub project_url:  String,
  pub commit_hash:  String,
}

impl BuildEvent {
  /// Capture a build event from the live build and project rows.
  #[must_use]
  pub fn from_build(
    build: &Build,
    project: &Project,
    commit_hash: &str,
  ) -> Self {
    Self {
      build_id:     build.id,
      status:       build.status,
      job_name:     build.job_name.clone(),
      drv_path:     build.drv_path.clone(),
      build_output: build.build_output_path.clone(),
      project_name: project.name.clone(),
      project_url:  project.repository_url.clone(),
      commit_hash:  commit_hash.to_string(),
    }
  }

  /// Whether this event represents a failed build (anything that is not a
  /// success). Used by channels honoring `on_failure_only`.
  #[must_use]
  pub const fn is_failure(&self) -> bool {
    !self.status.is_success()
  }

  /// Whether this event is an intermediate dependency build synthesized from
  /// the derivation graph rather than a top-level jobset job. Such builds are
  /// excluded from notification dispatch to avoid polluting commit statuses.
  #[must_use]
  pub fn is_dependency(&self) -> bool {
    self
      .job_name
      .starts_with(circus_common::models::DEPENDENCY_JOB_PREFIX)
  }

  /// Coarse status string for generic webhook payloads.
  #[must_use]
  pub const fn generic_status(&self) -> &'static str {
    match self.status {
      BuildStatus::Succeeded | BuildStatus::CachedFailure => "success",
      BuildStatus::Failed
      | BuildStatus::DependencyFailed
      | BuildStatus::FailedWithOutput
      | BuildStatus::Timeout
      | BuildStatus::LogLimitExceeded
      | BuildStatus::NarSizeLimitExceeded
      | BuildStatus::NonDeterministic
      | BuildStatus::OomKilled => "failure",
      BuildStatus::Cancelled => "cancelled",
      BuildStatus::Aborted => "aborted",
      BuildStatus::UnsupportedSystem => "skipped",
      BuildStatus::Pending | BuildStatus::Running => "pending",
    }
  }

  /// GitHub/Gitea commit-status `(state, description)` mapping.
  #[must_use]
  pub const fn github_state(&self) -> (&'static str, &'static str) {
    match self.status {
      BuildStatus::Succeeded | BuildStatus::CachedFailure => {
        ("success", "Build succeeded")
      },
      BuildStatus::Failed
      | BuildStatus::DependencyFailed
      | BuildStatus::FailedWithOutput
      | BuildStatus::NonDeterministic => ("failure", "Build failed"),
      BuildStatus::Running => ("pending", "Build in progress"),
      BuildStatus::Pending => ("pending", "Build queued"),
      BuildStatus::Cancelled => ("error", "Build cancelled"),
      BuildStatus::Aborted => ("error", "Build aborted"),
      BuildStatus::Timeout => ("error", "Build timed out"),
      BuildStatus::UnsupportedSystem => ("error", "Unsupported system"),
      BuildStatus::LogLimitExceeded => ("error", "Log limit exceeded"),
      BuildStatus::NarSizeLimitExceeded => ("error", "NAR size limit exceeded"),
      BuildStatus::OomKilled => ("failure", "Build failed (OOM)"),
    }
  }

  /// GitLab commit-status `(state, description)` mapping (distinct state
  /// vocabulary from GitHub/Gitea).
  #[must_use]
  pub const fn gitlab_state(&self) -> (&'static str, &'static str) {
    match self.status {
      BuildStatus::Succeeded | BuildStatus::CachedFailure => {
        ("success", "Build succeeded")
      },
      BuildStatus::Failed
      | BuildStatus::DependencyFailed
      | BuildStatus::FailedWithOutput
      | BuildStatus::NonDeterministic => ("failed", "Build failed"),
      BuildStatus::Running => ("running", "Build in progress"),
      BuildStatus::Pending => ("pending", "Build queued"),
      BuildStatus::Cancelled => ("canceled", "Build cancelled"),
      BuildStatus::Aborted => ("canceled", "Build aborted"),
      BuildStatus::Timeout => ("failed", "Build timed out"),
      BuildStatus::UnsupportedSystem => ("skipped", "Unsupported system"),
      BuildStatus::LogLimitExceeded => ("failed", "Log limit exceeded"),
      BuildStatus::NarSizeLimitExceeded => {
        ("failed", "NAR size limit exceeded")
      },
      BuildStatus::OomKilled => ("failed", "Build failed (OOM)"),
    }
  }

  /// Human-facing status label for email subjects/bodies.
  #[must_use]
  pub const fn email_status(&self) -> &'static str {
    match self.status {
      BuildStatus::Succeeded | BuildStatus::CachedFailure => "SUCCESS",
      BuildStatus::Failed
      | BuildStatus::DependencyFailed
      | BuildStatus::FailedWithOutput
      | BuildStatus::Timeout
      | BuildStatus::LogLimitExceeded
      | BuildStatus::NarSizeLimitExceeded
      | BuildStatus::NonDeterministic
      | BuildStatus::OomKilled => "FAILURE",
      BuildStatus::Cancelled => "CANCELLED",
      BuildStatus::Aborted => "ABORTED",
      BuildStatus::UnsupportedSystem => "UNSUPPORTED",
      BuildStatus::Pending | BuildStatus::Running => "PENDING",
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn event_with_job(job_name: &str) -> BuildEvent {
    BuildEvent {
      build_id:     Uuid::nil(),
      status:       BuildStatus::Succeeded,
      job_name:     job_name.to_string(),
      drv_path:     "/nix/store/x.drv".to_string(),
      build_output: None,
      project_name: "proj".to_string(),
      project_url:  "https://github.com/owner/repo".to_string(),
      commit_hash:  "abc".to_string(),
    }
  }

  #[test]
  fn top_level_job_is_not_a_dependency() {
    assert!(!event_with_job("hello").is_dependency());
  }

  #[test]
  fn drv_prefixed_job_is_a_dependency() {
    assert!(event_with_job("drv:abc-foo-1.0").is_dependency());
  }
}
