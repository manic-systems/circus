//! Read-only viewing pages: home, project detail, jobset detail,
//! evaluations, evaluation detail, builds, build detail, queue, channels,
//! channel detail, and starred.
//!
//! These handlers do not mutate server state; they only render templates.
//! Mutating admin actions live in `super::admin`.

use std::collections::BTreeSet;

use circus_common::models::BuildStatus;

use super::shared::status_badge;
use crate::operator;

mod queue;
pub(super) use queue::{Queue, QueueFilter, load as load_queue};

pub(super) fn is_job_name(name: &str) -> bool {
  !name.starts_with(circus_common::models::DEPENDENCY_JOB_PREFIX)
}

/// A dependency's derivation name, without the `drv:` prefix and store hash.
pub(super) fn derivation_name(job_name: &str) -> &str {
  job_name
    .strip_prefix(circus_common::models::DEPENDENCY_JOB_PREFIX)
    .map_or(job_name, |drv| {
      drv.split_once('-').map_or(drv, |(_, name)| name)
    })
}

pub(super) fn is_failed_status(status: BuildStatus) -> bool {
  status_badge(status).1 == "failed"
}

pub(super) const fn is_failed_derivation_status(status: BuildStatus) -> bool {
  matches!(
    status,
    BuildStatus::Failed
      | BuildStatus::FailedWithOutput
      | BuildStatus::Timeout
      | BuildStatus::CachedFailure
      | BuildStatus::LogLimitExceeded
      | BuildStatus::NarSizeLimitExceeded
      | BuildStatus::NonDeterministic
      | BuildStatus::OomKilled
  )
}

pub(super) fn dashboard_system_filters(
  overview: &operator::OperatorOverview,
) -> Vec<String> {
  let mut systems = BTreeSet::new();
  for build in &overview.recent_builds {
    if !build.system.is_empty() && build.system != "unknown" {
      systems.insert(build.system.clone());
    }
  }
  for item in &overview.queue_by_system {
    if !item.system.is_empty() {
      systems.insert(item.system.clone());
    }
  }
  for worker in &overview.workers {
    for system in worker.system.split(',').map(str::trim) {
      if !system.is_empty() && system != "-" {
        systems.insert(system.to_string());
      }
    }
  }
  for project in &overview.projects {
    for system in project.systems.split(',').map(str::trim) {
      if !system.is_empty() && system != "-" {
        systems.insert(system.to_string());
      }
    }
  }
  systems.into_iter().collect()
}

pub(super) fn format_elapsed(secs: i64) -> String {
  if secs < 60 {
    format!("{secs}s")
  } else if secs < 3600 {
    format!("{}m {}s", secs / 60, secs % 60)
  } else {
    format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
  }
}

pub(super) fn elapsed_since(started: i64) -> String {
  format_elapsed((jiff::Timestamp::now().as_second() - started).max(0))
}

#[cfg(test)]
mod tests {
  use circus_common::models::BuildStatus;

  use super::{is_failed_derivation_status, is_failed_status, is_job_name};

  #[test]
  fn job_lists_exclude_synthetic_dependency_names() {
    assert!(is_job_name("x86_64-linux.docs"));
    assert!(!is_job_name("drv:0vdd2i8j-intermediate"));
  }

  #[test]
  fn failed_derivations_exclude_dependency_failure_cascades() {
    assert!(is_failed_derivation_status(BuildStatus::Failed));
    assert!(is_failed_derivation_status(BuildStatus::OomKilled));
    assert!(!is_failed_derivation_status(BuildStatus::DependencyFailed));
    assert!(!is_failed_derivation_status(BuildStatus::Succeeded));
    assert!(!is_failed_derivation_status(BuildStatus::Running));
    assert!(!is_failed_derivation_status(BuildStatus::Cancelled));

    assert!(is_failed_status(BuildStatus::DependencyFailed));
  }
}
