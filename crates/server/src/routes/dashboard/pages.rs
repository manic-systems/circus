//! Read-only viewing pages: home, projects, project detail, jobset detail,
//! evaluations, evaluation detail, builds, build detail, queue, channels,
//! channel detail, starred, metrics, and the project-setup wizard.
//!
//! These handlers do not mutate server state; they only render templates.
//! Mutating admin actions live in `super::admin`.

use std::{
  cmp::Reverse,
  collections::{BTreeMap, BTreeSet, HashMap},
  path::Path as StdPath,
};

use axum::{
  extract::{Path, Query, State},
  http::StatusCode,
  response::{Html, IntoResponse, Response},
};
use circus_common::models::{Build, BuildStatus};
use tokio::fs;
use uuid::Uuid;

use super::{
  build_log::parse_build_log,
  shared::{
    BuildView,
    DashboardContext,
    DashboardPage,
    EvalSummaryView,
    JobStatusCell,
    JobStatusColumn,
    JobStatusRow,
    Pagination,
    ProjectSummaryView,
    QueueSystemView,
    RenderExt,
    WorkerSummaryView,
    build_view,
    build_view_with_context,
    enforce_page_access,
    eval_badge,
    eval_view,
    eval_view_with_context,
    not_found,
    status_badge,
  },
  templates::{
    BuildLogTemplate,
    BuildTemplate,
    BuildsTemplate,
    EvaluationTemplate,
    EvaluationsTemplate,
    HomeTemplate,
    JobsetJobsTemplate,
    JobsetTemplate,
    ProjectTemplate,
    ProjectsTemplate,
    UiTemplateConfig,
  },
};
use crate::{operator, state::AppState};

mod caches;
mod queue;
mod secondary;
pub(super) use caches::{cache_detail_page, cache_nars_page, caches_page};
pub(super) use queue::queue_page;
pub(super) use secondary::{
  channel_page,
  channels_page,
  metrics_page,
  project_setup_page,
  starred_page,
};

fn ui_config(state: &AppState) -> UiTemplateConfig {
  UiTemplateConfig::from_config(&state.config.ui)
}

fn is_job_name(name: &str) -> bool {
  !name.starts_with(circus_common::models::DEPENDENCY_JOB_PREFIX)
}

fn is_failed_status(status: BuildStatus) -> bool {
  status_badge(status).1 == "failed"
}

const fn is_failed_derivation_status(status: BuildStatus) -> bool {
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

fn dashboard_system_filters(
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

#[derive(serde::Deserialize)]
pub(super) struct BuildFilterParams {
  #[serde(
    default,
    deserialize_with = "crate::routes::serde_util::empty_string_as_none"
  )]
  status:   Option<String>,
  #[serde(
    default,
    deserialize_with = "crate::routes::serde_util::empty_string_as_none"
  )]
  system:   Option<String>,
  #[serde(
    default,
    deserialize_with = "crate::routes::serde_util::empty_string_as_none"
  )]
  job_name: Option<String>,
  limit:    Option<i64>,
  offset:   Option<i64>,
}

#[derive(serde::Deserialize)]
pub(super) struct JobsetJobsParams {
  show_inactive: Option<String>,
}

impl JobsetJobsParams {
  fn show_inactive(&self) -> bool {
    self
      .show_inactive
      .as_deref()
      .is_some_and(|v| matches!(v, "1" | "true" | "yes" | "on"))
  }
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

/// Render the dashboard landing page at `/`: aggregate build stats,
/// recent builds and evaluations, project summaries with last-eval
/// status, and announcements.
pub(super) async fn home(
  State(state): State<AppState>,
  ctx: DashboardContext,
) -> Result<Html<String>, Response> {
  enforce_page_access(&state.config, &ctx, DashboardPage::Home)?;
  let include_hidden = ctx.is_admin;
  let overview = operator::overview(&state, include_hidden)
    .await
    .map_err(IntoResponse::into_response)?;
  let evals = circus_common::repo::evaluations::list_filtered_with_visibility(
    &state.pool,
    None,
    None,
    5,
    0,
    include_hidden,
  )
  .await
  .unwrap_or_default();
  let announcements = circus_common::repo::news::list(&state.pool, 3, 0)
    .await
    .unwrap_or_default();

  let tmpl = HomeTemplate {
    ui: ui_config(&state),
    total_builds: overview.total_builds,
    completed_builds: overview.completed_builds,
    failed_builds: overview.failed_builds,
    running_builds: overview.running_builds,
    pending_builds: overview.pending_builds,
    recent_builds: overview.recent_builds.iter().map(BuildView::from).collect(),
    failed_builds_list: overview
      .failed_builds_list
      .iter()
      .map(BuildView::from)
      .collect(),
    recent_evals: evals.iter().map(eval_view).collect(),
    projects: overview
      .projects
      .iter()
      .map(ProjectSummaryView::from)
      .collect(),
    queue_by_system: overview
      .queue_by_system
      .iter()
      .map(QueueSystemView::from)
      .collect(),
    workers: overview
      .workers
      .iter()
      .map(WorkerSummaryView::from)
      .collect(),
    system_filters: dashboard_system_filters(&overview),
    worker_online: overview.worker_online,
    worker_total: overview.worker_total,
    refreshed_at: overview.refreshed_at,
    announcements,
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name.clone(),
  };
  tmpl.render_html_or_500()
}

/// Render the paginated project list at `/projects`.
pub(super) async fn projects_page(
  State(state): State<AppState>,
  Query(params): Query<PageParams>,
  ctx: DashboardContext,
) -> Result<Html<String>, Response> {
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

pub(super) async fn project_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  ctx: DashboardContext,
) -> Result<Html<String>, Response> {
  enforce_page_access(&state.config, &ctx, DashboardPage::Project)?;
  let include_hidden = ctx.is_admin;
  let Ok(project) = circus_common::repo::projects::get(&state.pool, id).await
  else {
    return Err(not_found("Project"));
  };
  let jobsets =
    circus_common::repo::jobsets::list_for_project(&state.pool, id, 100, 0)
      .await
      .unwrap_or_default();

  // Get evaluations for this project's jobsets
  let mut evals = Vec::new();
  for js in &jobsets {
    let mut js_evals =
      circus_common::repo::evaluations::list_filtered_with_visibility(
        &state.pool,
        Some(js.id),
        None,
        5,
        0,
        include_hidden,
      )
      .await
      .unwrap_or_default();
    evals.append(&mut js_evals);
  }
  evals.sort_by_key(|e| Reverse(e.evaluation_time));
  evals.truncate(10);

  let tmpl = ProjectTemplate {
    ui: ui_config(&state),
    project,
    jobsets,
    recent_evals: evals.iter().map(eval_view).collect(),
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name.clone(),
    csrf_token: ctx.csrf_token.clone(),
  };
  tmpl.render_html_or_500()
}

pub(super) async fn jobset_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  ctx: DashboardContext,
) -> Result<Html<String>, Response> {
  enforce_page_access(&state.config, &ctx, DashboardPage::Jobset)?;
  let include_hidden = ctx.is_admin;
  let Ok(jobset) = circus_common::repo::jobsets::get(&state.pool, id).await
  else {
    return Err(not_found("Jobset"));
  };
  let Ok(project) =
    circus_common::repo::projects::get(&state.pool, jobset.project_id).await
  else {
    return Err(not_found("Project"));
  };

  let evals = circus_common::repo::evaluations::list_filtered_with_visibility(
    &state.pool,
    Some(id),
    None,
    20,
    0,
    include_hidden,
  )
  .await
  .unwrap_or_default();

  let eval_ids: Vec<Uuid> = evals.iter().map(|e| e.id).collect();
  let builds = circus_common::repo::builds::list_for_jobset_evaluations(
    &state.pool,
    id,
    &eval_ids,
  )
  .await
  .unwrap_or_default();

  let mut builds_by_eval: HashMap<Uuid, Vec<&Build>> = HashMap::new();
  for b in builds.iter().filter(|build| is_job_name(&build.job_name)) {
    builds_by_eval.entry(b.evaluation_id).or_default().push(b);
  }

  let mut summaries = Vec::new();
  for e in &evals {
    let (text, class) = eval_badge(&e.status);
    let short = if e.commit_hash.len() > 12 {
      e.commit_hash[..12].to_string()
    } else {
      e.commit_hash.clone()
    };

    let eval_builds =
      builds_by_eval.get(&e.id).map_or_else(|| &[], Vec::as_slice);
    let succeeded = eval_builds
      .iter()
      .filter(|b| b.status == BuildStatus::Succeeded)
      .count() as i64;
    let failed = eval_builds
      .iter()
      .filter(|b| {
        matches!(
          b.status,
          BuildStatus::Failed
            | BuildStatus::DependencyFailed
            | BuildStatus::FailedWithOutput
            | BuildStatus::Timeout
            | BuildStatus::CachedFailure
            | BuildStatus::LogLimitExceeded
            | BuildStatus::NarSizeLimitExceeded
            | BuildStatus::NonDeterministic
        )
      })
      .count() as i64;
    let pending = eval_builds
      .iter()
      .filter(|b| b.status == BuildStatus::Pending)
      .count() as i64;

    summaries.push(EvalSummaryView {
      id: e.id,
      commit_short: short,
      status_text: text,
      status_class: class,
      time: e.evaluation_time.format("%Y-%m-%d %H:%M").to_string(),
      succeeded,
      failed,
      pending,
      hidden: e.hidden,
    });
  }

  let tmpl = JobsetTemplate {
    ui: ui_config(&state),
    project,
    jobset,
    eval_summaries: summaries,
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name.clone(),
    csrf_token: ctx.csrf_token.clone(),
  };
  tmpl.render_html_or_500()
}

pub(super) async fn jobset_jobs_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  Query(params): Query<JobsetJobsParams>,
  ctx: DashboardContext,
) -> Result<Html<String>, Response> {
  enforce_page_access(&state.config, &ctx, DashboardPage::JobsetJobs)?;
  let include_hidden = ctx.is_admin;
  let Ok(jobset) = circus_common::repo::jobsets::get(&state.pool, id).await
  else {
    return Err(not_found("Jobset"));
  };
  let Ok(project) =
    circus_common::repo::projects::get(&state.pool, jobset.project_id).await
  else {
    return Err(not_found("Project"));
  };

  let evals = circus_common::repo::evaluations::list_filtered_with_visibility(
    &state.pool,
    Some(id),
    None,
    20,
    0,
    include_hidden,
  )
  .await
  .unwrap_or_default();
  let eval_ids: Vec<Uuid> = evals.iter().map(|e| e.id).collect();
  let latest_eval_id = eval_ids.first().copied();
  let builds = circus_common::repo::builds::list_for_jobset_evaluations(
    &state.pool,
    id,
    &eval_ids,
  )
  .await
  .unwrap_or_default();

  let columns: Vec<JobStatusColumn> = evals
    .iter()
    .map(|e| {
      let commit_short = if e.commit_hash.len() > 12 {
        e.commit_hash[..12].to_string()
      } else {
        e.commit_hash.clone()
      };
      let hidden_suffix = if e.hidden { " (hidden)" } else { "" };
      JobStatusColumn {
        eval_id: e.id,
        label:   e.evaluation_time.format("%m-%d %H:%M").to_string(),
        title:   format!("{commit_short}{hidden_suffix}"),
      }
    })
    .collect();

  let mut builds_by_job: BTreeMap<String, HashMap<Uuid, Build>> =
    BTreeMap::new();
  for build in builds
    .into_iter()
    .filter(|build| is_job_name(&build.job_name))
  {
    builds_by_job
      .entry(build.job_name.clone())
      .or_default()
      .insert(build.evaluation_id, build);
  }

  let show_inactive = params.show_inactive();
  let mut rows = Vec::new();
  for (job_name, by_eval) in builds_by_job {
    let is_active =
      latest_eval_id.is_some_and(|eval_id| by_eval.contains_key(&eval_id));
    if !show_inactive && !is_active {
      continue;
    }
    let cells = columns
      .iter()
      .map(|column| {
        by_eval.get(&column.eval_id).map_or_else(
          || {
            JobStatusCell {
              href:         String::new(),
              status_text:  "-".to_string(),
              status_class: "skipped".to_string(),
            }
          },
          |build| {
            let (status_text, status_class) = status_badge(build.status);
            JobStatusCell {
              href: format!("/build/{}", build.id),
              status_text,
              status_class,
            }
          },
        )
      })
      .collect();
    rows.push(JobStatusRow {
      job_name,
      is_active,
      cells,
    });
  }

  let tmpl = JobsetJobsTemplate {
    ui: ui_config(&state),
    project,
    jobset,
    columns,
    rows,
    show_inactive,
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name.clone(),
  };
  tmpl.render_html_or_500()
}

/// Render the paginated evaluation list at `/evaluations`, enriched with
/// the owning project and jobset names. Hidden evaluations are included
/// only for admins.
pub(super) async fn evaluations_page(
  State(state): State<AppState>,
  Query(params): Query<PageParams>,
  ctx: DashboardContext,
) -> Result<Html<String>, Response> {
  enforce_page_access(&state.config, &ctx, DashboardPage::Evaluations)?;
  let include_hidden = ctx.is_admin;
  let limit = params.limit.unwrap_or(50).clamp(1, 200);
  let offset = params.offset.unwrap_or(0).max(0);
  let items = circus_common::repo::evaluations::list_filtered_with_visibility(
    &state.pool,
    None,
    None,
    limit,
    offset,
    include_hidden,
  )
  .await
  .unwrap_or_default();
  let total = circus_common::repo::evaluations::count_filtered_with_visibility(
    &state.pool,
    None,
    None,
    include_hidden,
  )
  .await
  .unwrap_or(0);

  // Enrich evaluations with jobset/project names
  let mut enriched = Vec::new();
  for e in &items {
    let (jname, pname) =
      match circus_common::repo::jobsets::get(&state.pool, e.jobset_id).await {
        Ok(js) => {
          let pname =
            circus_common::repo::projects::get(&state.pool, js.project_id)
              .await
              .map_or_else(|_| "-".to_string(), |p| p.name);
          (js.name, pname)
        },
        Err(_) => ("-".to_string(), "-".to_string()),
      };
    enriched.push(eval_view_with_context(e, &jname, &pname));
  }

  let pagination = Pagination::new(total, offset, limit);
  let tmpl = EvaluationsTemplate {
    ui: ui_config(&state),
    evals: enriched,
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

pub(super) async fn evaluation_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  ctx: DashboardContext,
) -> Result<Html<String>, Response> {
  enforce_page_access(&state.config, &ctx, DashboardPage::Evaluation)?;
  let include_hidden = ctx.is_admin;
  let Ok(eval) = circus_common::repo::evaluations::get_visible(
    &state.pool,
    id,
    include_hidden,
  )
  .await
  else {
    return Err(not_found("Evaluation"));
  };

  let Ok(jobset) =
    circus_common::repo::jobsets::get(&state.pool, eval.jobset_id).await
  else {
    return Err(not_found("Jobset"));
  };
  let Ok(project) =
    circus_common::repo::projects::get(&state.pool, jobset.project_id).await
  else {
    return Err(not_found("Project"));
  };

  let builds =
    circus_common::repo::builds::list_for_evaluation(&state.pool, id)
      .await
      .unwrap_or_default();

  let top_level_builds = builds
    .iter()
    .filter(|build| is_job_name(&build.job_name))
    .collect::<Vec<_>>();
  let failed_derivations = builds
    .iter()
    .filter(|build| is_failed_derivation_status(build.status))
    .map(build_view)
    .collect();

  let succeeded = top_level_builds
    .iter()
    .filter(|b| b.status == BuildStatus::Succeeded)
    .count() as i64;
  let failed = top_level_builds
    .iter()
    .filter(|b| is_failed_status(b.status))
    .count() as i64;
  let running = top_level_builds
    .iter()
    .filter(|b| b.status == BuildStatus::Running)
    .count() as i64;
  let pending = top_level_builds
    .iter()
    .filter(|b| b.status == BuildStatus::Pending)
    .count() as i64;

  let tmpl = EvaluationTemplate {
    ui: ui_config(&state),
    eval: eval_view(&eval),
    builds: top_level_builds.into_iter().map(build_view).collect(),
    failed_derivations,
    project_name: project.name,
    project_id: project.id,
    jobset_name: jobset.name,
    jobset_id: jobset.id,
    succeeded_count: succeeded,
    failed_count: failed,
    running_count: running,
    pending_count: pending,
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name.clone(),
    csrf_token: ctx.csrf_token.clone(),
  };
  tmpl.render_html_or_500()
}

/// Render the filterable build listing at `/builds`. Each row is
/// enriched with its owning project and jobset; the filter form drives
/// pagination via `BuildFilterParams`.
pub(super) async fn builds_page(
  State(state): State<AppState>,
  Query(params): Query<BuildFilterParams>,
  ctx: DashboardContext,
) -> Result<Html<String>, Response> {
  enforce_page_access(&state.config, &ctx, DashboardPage::Builds)?;
  let limit = params.limit.unwrap_or(50).clamp(1, 200);
  let offset = params.offset.unwrap_or(0).max(0);
  let items = circus_common::repo::builds::list_filtered(
    &state.pool,
    None,
    params.status.as_deref(),
    params.system.as_deref(),
    params.job_name.as_deref(),
    limit,
    offset,
  )
  .await
  .unwrap_or_default();
  let total = circus_common::repo::builds::count_filtered(
    &state.pool,
    None,
    params.status.as_deref(),
    params.system.as_deref(),
    params.job_name.as_deref(),
  )
  .await
  .unwrap_or(0);

  let pagination = Pagination::new(total, offset, limit);

  let mut context_by_eval = HashMap::new();
  for item in &items {
    if context_by_eval.contains_key(&item.evaluation_id) {
      continue;
    }
    let context = match circus_common::repo::evaluations::get(
      &state.pool,
      item.evaluation_id,
    )
    .await
    {
      Ok(eval) => {
        match circus_common::repo::jobsets::get(&state.pool, eval.jobset_id)
          .await
        {
          Ok(jobset) => {
            match circus_common::repo::projects::get(
              &state.pool,
              jobset.project_id,
            )
            .await
            {
              Ok(project) => {
                Some((project.id, project.name, jobset.id, jobset.name))
              },
              Err(_) => None,
            }
          },
          Err(_) => None,
        }
      },
      Err(_) => None,
    };
    if let Some(context) = context {
      context_by_eval.insert(item.evaluation_id, context);
    }
  }

  let tmpl = BuildsTemplate {
    ui: ui_config(&state),
    builds: items
      .iter()
      .map(|item| {
        context_by_eval.get(&item.evaluation_id).map_or_else(
          || build_view(item),
          |(project_id, project_name, jobset_id, jobset_name)| {
            build_view_with_context(
              item,
              *project_id,
              project_name,
              *jobset_id,
              jobset_name,
            )
          },
        )
      })
      .collect(),
    limit,
    has_prev: pagination.has_prev,
    has_next: pagination.has_next,
    prev_offset: pagination.prev_offset,
    next_offset: pagination.next_offset,
    page: pagination.page,
    total_pages: pagination.total_pages,
    filter_status: params.status.unwrap_or_default(),
    filter_system: params.system.unwrap_or_default(),
    filter_job: params.job_name.unwrap_or_default(),
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name.clone(),
  };
  tmpl.render_html_or_500()
}

pub(super) async fn build_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  ctx: DashboardContext,
) -> Result<Html<String>, Response> {
  enforce_page_access(&state.config, &ctx, DashboardPage::Build)?;
  let Ok(build) = circus_common::repo::builds::get(&state.pool, id).await
  else {
    return Err(not_found("Build"));
  };

  let Ok(eval) =
    circus_common::repo::evaluations::get(&state.pool, build.evaluation_id)
      .await
  else {
    return Err(not_found("Evaluation"));
  };
  let Ok(jobset) =
    circus_common::repo::jobsets::get(&state.pool, eval.jobset_id).await
  else {
    return Err(not_found("Jobset"));
  };
  let Ok(project) =
    circus_common::repo::projects::get(&state.pool, jobset.project_id).await
  else {
    return Err(not_found("Project"));
  };

  let eval_commit_short = if eval.commit_hash.len() > 12 {
    eval.commit_hash[..12].to_string()
  } else {
    eval.commit_hash.clone()
  };

  let steps = circus_common::repo::build_steps::list_for_build(&state.pool, id)
    .await
    .unwrap_or_default();
  let products =
    circus_common::repo::build_products::list_for_build(&state.pool, id)
      .await
      .unwrap_or_default();
  let dependencies =
    circus_common::repo::build_dependencies::list_dependency_builds(
      &state.pool,
      id,
    )
    .await
    .unwrap_or_default()
    .iter()
    .map(build_view)
    .collect();
  let dependents =
    circus_common::repo::build_dependencies::list_dependent_builds(
      &state.pool,
      id,
    )
    .await
    .unwrap_or_default()
    .iter()
    .map(build_view)
    .collect();

  // Resolve who ran the build
  let builder_label = if let Some(machine_id) = build.agent_machine_id {
    circus_common::repo::builder_sessions::get(&state.pool, machine_id)
      .await
      .map_or_else(|_| "local".to_string(), |s| s.name)
  } else if let Some(builder_id) = build.builder_id {
    circus_common::repo::remote_builders::get(&state.pool, builder_id)
      .await
      .map_or_else(|_| "local".to_string(), |b| b.name)
  } else {
    "local".to_string()
  };

  let tmpl = BuildTemplate {
    ui: ui_config(&state),
    build: build_view(&build),
    builder_label,
    steps,
    products,
    dependencies,
    dependents,
    eval_id: eval.id,
    eval_commit_short,
    jobset_id: jobset.id,
    jobset_name: jobset.name,
    project_id: project.id,
    project_name: project.name,
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name.clone(),
  };
  tmpl.render_html_or_500()
}

/// Render a build's full log as a structured dashboard page.
pub(super) async fn build_log(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  ctx: DashboardContext,
) -> Result<Response, Response> {
  enforce_page_access(&state.config, &ctx, DashboardPage::Build)?;

  let Ok(build) = circus_common::repo::builds::get(&state.pool, id).await
  else {
    return Ok((StatusCode::NOT_FOUND, "Build not found").into_response());
  };
  if circus_common::repo::evaluations::get_visible(
    &state.pool,
    build.evaluation_id,
    ctx.is_admin,
  )
  .await
  .is_err()
  {
    return Ok((StatusCode::NOT_FOUND, "Build not found").into_response());
  }

  let Ok(eval) =
    circus_common::repo::evaluations::get(&state.pool, build.evaluation_id)
      .await
  else {
    return Ok((StatusCode::NOT_FOUND, "Evaluation not found").into_response());
  };
  let Ok(jobset) =
    circus_common::repo::jobsets::get(&state.pool, eval.jobset_id).await
  else {
    return Ok((StatusCode::NOT_FOUND, "Jobset not found").into_response());
  };
  let Ok(project) =
    circus_common::repo::projects::get(&state.pool, jobset.project_id).await
  else {
    return Ok((StatusCode::NOT_FOUND, "Project not found").into_response());
  };

  let Some(path) = build.log_path.as_deref().filter(|p| !p.is_empty()) else {
    return Ok(
      (StatusCode::NOT_FOUND, "No log for this build").into_response(),
    );
  };
  let Some(path) = crate::routes::canonical_log_file(
    &state.config.logs.log_dir,
    StdPath::new(path),
  )
  .await
  else {
    return Ok(
      (StatusCode::NOT_FOUND, "Log file is unavailable").into_response(),
    );
  };

  let Ok(raw) = fs::read_to_string(path).await else {
    return Ok(
      (StatusCode::NOT_FOUND, "Log file is unavailable").into_response(),
    );
  };

  let eval_commit_short = if eval.commit_hash.len() > 12 {
    eval.commit_hash[..12].to_string()
  } else {
    eval.commit_hash.clone()
  };
  let tmpl = BuildLogTemplate {
    ui: ui_config(&state),
    build: build_view(&build),
    log: parse_build_log(&raw),
    eval_id: eval.id,
    eval_commit_short,
    jobset_id: jobset.id,
    jobset_name: jobset.name,
    project_id: project.id,
    project_name: project.name,
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name.clone(),
  };
  tmpl.render_html_or_500().map(IntoResponse::into_response)
}

#[cfg(test)]
mod tests {
  use circus_common::models::BuildStatus;

  use super::{
    BuildFilterParams,
    is_failed_derivation_status,
    is_failed_status,
    is_job_name,
  };

  #[test]
  fn blank_filter_params_deserialize_to_none() {
    let params = serde_urlencoded::from_str::<BuildFilterParams>(
      "offset=50&limit=50&status=&system=&job_name=",
    )
    .expect("deserialize query");
    assert_eq!(params.status, None);
    assert_eq!(params.system, None);
    assert_eq!(params.job_name, None);
    assert_eq!(params.offset, Some(50));

    let kept = serde_urlencoded::from_str::<BuildFilterParams>("status=failed")
      .expect("deserialize query");
    assert_eq!(kept.status.as_deref(), Some("failed"));
  }

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
