//! Read-only viewing pages: home, projects, project detail, jobset detail,
//! evaluations, evaluation detail, builds, build detail, queue, channels,
//! channel detail, starred, metrics, and the project-setup wizard.
//!
//! These handlers do not mutate server state; they only render templates.
//! Mutating admin actions live in `super::admin`.

use std::collections::BTreeSet;

use axum::{
  extract::{Query, State},
  response::Html,
};
use circus_common::models::BuildStatus;

use super::{
  shared::{
    DashboardContext,
    DashboardPage,
    PageError,
    Pagination,
    RenderExt,
    enforce_page_access,
    status_badge,
  },
  templates::{ProjectsTemplate, UiTemplateConfig},
};
use crate::{operator, state::AppState};

mod caches;
mod queue;
mod secondary;
pub(super) use caches::cache_detail_page;
pub(super) use queue::{Queue, QueueFilter, load as load_queue};
pub(super) use secondary::{metrics_page, project_setup_page};

fn ui_config(state: &AppState) -> UiTemplateConfig {
  UiTemplateConfig::from_config(&state.config.ui)
}

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

#[derive(serde::Deserialize)]
pub(super) struct PageParams {
  pub(super) limit:  Option<i64>,
  pub(super) offset: Option<i64>,
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

/// Render the paginated project list at `/projects`.
pub(super) async fn projects_page(
  State(state): State<AppState>,
  Query(params): Query<PageParams>,
  ctx: DashboardContext,
) -> Result<Html<String>, PageError> {
  enforce_page_access(&state.config, &ctx, DashboardPage::Projects)?;
  let limit = params.limit.unwrap_or(50).clamp(1, 200);
  let offset = params.offset.unwrap_or(0).max(0);
  let items = circus_common::repo::projects::list(&state.pool, limit, offset)
    .await
    .unwrap_or_default();
  let total = circus_common::repo::projects::count(&state.pool)
    .await
    .unwrap_or(0);

  let pagination = Pagination::new(total, offset, limit);
  let tmpl = ProjectsTemplate {
    ui: ui_config(&state),
    projects: items,
    limit,
    has_prev: pagination.has_prev,
    has_next: pagination.has_next,
    prev_offset: pagination.prev_offset,
    next_offset: pagination.next_offset,
    page: pagination.page,
    total_pages: pagination.total_pages,
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name.clone(),
    csrf_token: ctx.csrf_token.clone(),
  };
  tmpl.render_html_or_500()
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
