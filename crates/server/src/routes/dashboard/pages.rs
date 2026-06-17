//! Read-only viewing pages: home, projects, project detail, jobset detail,
//! evaluations, evaluation detail, builds, build detail, queue, channels,
//! channel detail, starred, metrics, and the project-setup wizard.
//!
//! These handlers do not mutate server state; they only render templates.
//! Mutating admin actions live in `super::admin`.

use std::collections::HashMap;

use axum::{
  extract::{Path, Query, State},
  http::{Extensions, StatusCode, header},
  response::{Html, IntoResponse, Redirect, Response},
};
use circus_common::models::{BuildStatus, Evaluation};
use uuid::Uuid;

use super::{
  shared::{
    DashboardPage,
    EvalSummaryView,
    JobStatusCell,
    JobStatusColumn,
    JobStatusRow,
    Pagination,
    ProjectSummaryView,
    QueueBuildView,
    RenderExt,
    StarredJobView,
    auth_name,
    build_view,
    build_view_with_context,
    decode_build_log,
    enforce_page_access,
    eval_badge,
    eval_view,
    eval_view_with_context,
    is_admin,
    status_badge,
  },
  templates::{
    BuildTemplate,
    BuildsTemplate,
    ChannelTemplate,
    ChannelsTemplate,
    EvaluationTemplate,
    EvaluationsTemplate,
    HomeTemplate,
    JobsetJobsTemplate,
    JobsetTemplate,
    MetricsTemplate,
    ProjectSetupTemplate,
    ProjectTemplate,
    ProjectsTemplate,
    QueueTemplate,
    StarredTemplate,
  },
};
use crate::{permissions::UiPermissions, state::AppState};

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
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  enforce_page_access(&state.config.server, &extensions, DashboardPage::Home)?;
  let include_hidden = is_admin(&extensions);
  let build_stats = circus_common::repo::builds::get_stats(&state.pool)
    .await
    .unwrap_or_default();
  let builds = circus_common::repo::builds::list_recent(&state.pool, 10)
    .await
    .unwrap_or_default();
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

  // Fetch project summaries
  let all_projects = circus_common::repo::projects::list(&state.pool, 10, 0)
    .await
    .unwrap_or_default();
  let mut project_summaries = Vec::new();
  for p in &all_projects {
    let jobset_count =
      circus_common::repo::jobsets::count_for_project(&state.pool, p.id)
        .await
        .unwrap_or(0);
    let jobsets =
      circus_common::repo::jobsets::list_for_project(&state.pool, p.id, 100, 0)
        .await
        .unwrap_or_default();
    let mut last_eval: Option<Evaluation> = None;
    for js in &jobsets {
      let js_evals =
        circus_common::repo::evaluations::list_filtered_with_visibility(
          &state.pool,
          Some(js.id),
          None,
          1,
          0,
          include_hidden,
        )
        .await
        .unwrap_or_default();
      if let Some(e) = js_evals.into_iter().next()
        && last_eval
          .as_ref()
          .is_none_or(|le| e.evaluation_time > le.evaluation_time)
      {
        last_eval = Some(e);
      }
    }
    let (status, class, time) = last_eval.as_ref().map_or_else(
      || ("-".into(), "pending".into(), "-".into()),
      |e| {
        let (t, c) = eval_badge(&e.status);
        (t, c, e.evaluation_time.format("%Y-%m-%d %H:%M").to_string())
      },
    );
    project_summaries.push(ProjectSummaryView {
      id: p.id,
      name: p.name.clone(),
      jobset_count,
      last_eval_status: status,
      last_eval_class: class,
      last_eval_time: time,
    });
  }

  let tmpl = HomeTemplate {
    total_builds: build_stats.total_builds.unwrap_or(0),
    completed_builds: build_stats.completed_builds.unwrap_or(0),
    failed_builds: build_stats.failed_builds.unwrap_or(0),
    running_builds: build_stats.running_builds.unwrap_or(0),
    pending_builds: build_stats.pending_builds.unwrap_or(0),
    recent_builds: builds.iter().map(build_view).collect(),
    recent_evals: evals.iter().map(eval_view).collect(),
    projects: project_summaries,
    announcements,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  tmpl.render_html_or_500()
}

/// Render the paginated project list at `/projects`.
pub(super) async fn projects_page(
  State(state): State<AppState>,
  Query(params): Query<PageParams>,
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  enforce_page_access(
    &state.config.server,
    &extensions,
    DashboardPage::Projects,
  )?;
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
    projects: items,
    limit,
    has_prev: pagination.has_prev,
    has_next: pagination.has_next,
    prev_offset: pagination.prev_offset,
    next_offset: pagination.next_offset,
    page: pagination.page,
    total_pages: pagination.total_pages,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
    csrf_token: super::csrf::csrf_from(&extensions),
  };
  tmpl.render_html_or_500()
}

pub(super) async fn project_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  enforce_page_access(
    &state.config.server,
    &extensions,
    DashboardPage::Project,
  )?;
  let include_hidden = is_admin(&extensions);
  let Ok(project) = circus_common::repo::projects::get(&state.pool, id).await
  else {
    return Ok(Html("Project not found".to_string()));
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
  evals.sort_by_key(|e| std::cmp::Reverse(e.evaluation_time));
  evals.truncate(10);

  let tmpl = ProjectTemplate {
    project,
    jobsets,
    recent_evals: evals.iter().map(eval_view).collect(),
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
    csrf_token: super::csrf::csrf_from(&extensions),
  };
  tmpl.render_html_or_500()
}

pub(super) async fn jobset_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  enforce_page_access(
    &state.config.server,
    &extensions,
    DashboardPage::Jobset,
  )?;
  let include_hidden = is_admin(&extensions);
  let Ok(jobset) = circus_common::repo::jobsets::get(&state.pool, id).await
  else {
    return Ok(Html("Jobset not found".to_string()));
  };
  let Ok(project) =
    circus_common::repo::projects::get(&state.pool, jobset.project_id).await
  else {
    return Ok(Html("Project not found".to_string()));
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

  let mut builds_by_eval: HashMap<Uuid, Vec<&circus_common::models::Build>> =
    HashMap::new();
  for b in &builds {
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
    project,
    jobset,
    eval_summaries: summaries,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
    csrf_token: super::csrf::csrf_from(&extensions),
  };
  tmpl.render_html_or_500()
}

pub(super) async fn jobset_jobs_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  Query(params): Query<JobsetJobsParams>,
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  enforce_page_access(
    &state.config.server,
    &extensions,
    DashboardPage::JobsetJobs,
  )?;
  let include_hidden = is_admin(&extensions);
  let Ok(jobset) = circus_common::repo::jobsets::get(&state.pool, id).await
  else {
    return Ok(Html("Jobset not found".to_string()));
  };
  let Ok(project) =
    circus_common::repo::projects::get(&state.pool, jobset.project_id).await
  else {
    return Ok(Html("Project not found".to_string()));
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

  let mut builds_by_job: std::collections::BTreeMap<
    String,
    std::collections::HashMap<Uuid, circus_common::models::Build>,
  > = std::collections::BTreeMap::new();
  for build in builds {
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
    project,
    jobset,
    columns,
    rows,
    show_inactive,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  tmpl.render_html_or_500()
}

/// Render the paginated evaluation list at `/evaluations`, enriched with
/// the owning project and jobset names. Hidden evaluations are included
/// only for admins.
pub(super) async fn evaluations_page(
  State(state): State<AppState>,
  Query(params): Query<PageParams>,
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  enforce_page_access(
    &state.config.server,
    &extensions,
    DashboardPage::Evaluations,
  )?;
  let include_hidden = is_admin(&extensions);
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
    evals: enriched,
    limit,
    has_prev: pagination.has_prev,
    has_next: pagination.has_next,
    prev_offset: pagination.prev_offset,
    next_offset: pagination.next_offset,
    page: pagination.page,
    total_pages: pagination.total_pages,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
    csrf_token: super::csrf::csrf_from(&extensions),
  };
  tmpl.render_html_or_500()
}

pub(super) async fn evaluation_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  enforce_page_access(
    &state.config.server,
    &extensions,
    DashboardPage::Evaluation,
  )?;
  let include_hidden = is_admin(&extensions);
  let Ok(eval) = circus_common::repo::evaluations::get_visible(
    &state.pool,
    id,
    include_hidden,
  )
  .await
  else {
    return Ok(Html("Evaluation not found".to_string()));
  };

  let Ok(jobset) =
    circus_common::repo::jobsets::get(&state.pool, eval.jobset_id).await
  else {
    return Ok(Html("Jobset not found".to_string()));
  };
  let Ok(project) =
    circus_common::repo::projects::get(&state.pool, jobset.project_id).await
  else {
    return Ok(Html("Project not found".to_string()));
  };

  let builds = circus_common::repo::builds::list_filtered(
    &state.pool,
    Some(id),
    None,
    None,
    None,
    200,
    0,
  )
  .await
  .unwrap_or_default();

  let succeeded = builds
    .iter()
    .filter(|b| b.status == BuildStatus::Succeeded)
    .count() as i64;
  let failed = builds
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
  let running = builds
    .iter()
    .filter(|b| b.status == BuildStatus::Running)
    .count() as i64;
  let pending = builds
    .iter()
    .filter(|b| b.status == BuildStatus::Pending)
    .count() as i64;

  let tmpl = EvaluationTemplate {
    eval:            eval_view(&eval),
    builds:          builds.iter().map(build_view).collect(),
    project_name:    project.name,
    project_id:      project.id,
    jobset_name:     jobset.name,
    jobset_id:       jobset.id,
    succeeded_count: succeeded,
    failed_count:    failed,
    running_count:   running,
    pending_count:   pending,
    is_admin:        is_admin(&extensions),
    auth_name:       auth_name(&extensions),
    csrf_token:      super::csrf::csrf_from(&extensions),
  };
  tmpl.render_html_or_500()
}

/// Render the filterable build listing at `/builds`. Each row is
/// enriched with its owning project and jobset; the filter form drives
/// pagination via `BuildFilterParams`.
pub(super) async fn builds_page(
  State(state): State<AppState>,
  Query(params): Query<BuildFilterParams>,
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  enforce_page_access(
    &state.config.server,
    &extensions,
    DashboardPage::Builds,
  )?;
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

  let mut context_by_eval = std::collections::HashMap::new();
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
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  tmpl.render_html_or_500()
}

pub(super) async fn build_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  enforce_page_access(&state.config.server, &extensions, DashboardPage::Build)?;
  let Ok(build) = circus_common::repo::builds::get(&state.pool, id).await
  else {
    return Ok(Html("Build not found".to_string()));
  };

  let Ok(eval) =
    circus_common::repo::evaluations::get(&state.pool, build.evaluation_id)
      .await
  else {
    return Ok(Html("Evaluation not found".to_string()));
  };
  let Ok(jobset) =
    circus_common::repo::jobsets::get(&state.pool, eval.jobset_id).await
  else {
    return Ok(Html("Jobset not found".to_string()));
  };
  let Ok(project) =
    circus_common::repo::projects::get(&state.pool, jobset.project_id).await
  else {
    return Ok(Html("Project not found".to_string()));
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
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  tmpl.render_html_or_500()
}

/// Serve a build's full log as plain text.
pub(super) async fn build_log(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  extensions: Extensions,
) -> Result<Response, Response> {
  enforce_page_access(&state.config.server, &extensions, DashboardPage::Build)?;

  let Ok(build) = circus_common::repo::builds::get(&state.pool, id).await
  else {
    return Ok((StatusCode::NOT_FOUND, "Build not found").into_response());
  };

  let Some(path) = build.log_path.as_deref().filter(|p| !p.is_empty()) else {
    return Ok(
      (StatusCode::NOT_FOUND, "No log for this build").into_response(),
    );
  };
  let Some(path) = crate::routes::canonical_log_file(
    &state.config.logs.log_dir,
    std::path::Path::new(path),
  )
  .await
  else {
    return Ok(
      (StatusCode::NOT_FOUND, "Log file is unavailable").into_response(),
    );
  };

  let Ok(raw) = tokio::fs::read_to_string(path).await else {
    return Ok(
      (StatusCode::NOT_FOUND, "Log file is unavailable").into_response(),
    );
  };

  Ok(
    (
      [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
      decode_build_log(&raw),
    )
      .into_response(),
  )
}

/// Render the build queue at `/queue`: running builds with live elapsed
/// timers, and pending builds in scheduler order. Each row carries its
/// project and jobset, and pending rows expose a "push forward" form
/// when the session's [`UiPermissions`] allow it.
pub(super) async fn queue_page(
  State(state): State<AppState>,
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  enforce_page_access(&state.config.server, &extensions, DashboardPage::Queue)?;
  let running = circus_common::repo::builds::list_filtered(
    &state.pool,
    None,
    Some("running"),
    None,
    None,
    100,
    0,
  )
  .await
  .unwrap_or_default();
  // Order pending by the same key the queue runner uses (priority DESC,
  // created_at ASC) so the displayed queue position matches what the
  // scheduler will pick next, and a "Push forward" bump visibly moves
  // the build up the list.
  let pending = circus_common::repo::builds::list_pending_in_scheduler_order(
    &state.pool,
    100,
    0,
  )
  .await
  .unwrap_or_default();

  // Build builder ID -> name map
  let builders = circus_common::repo::remote_builders::list(&state.pool)
    .await
    .unwrap_or_default();
  let builder_map: std::collections::HashMap<Uuid, String> =
    builders.into_iter().map(|b| (b.id, b.name)).collect();

  // Agent machine_id -> name map
  let agent_map = circus_common::repo::builder_sessions::list(&state.pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|s| (s.machine_id, s.name))
    .collect::<HashMap<Uuid, String>>();

  // Resolve each evaluation_id appearing in either list to its
  // (project_id, project_name, jobset_id, jobset_name). Cache so each unique
  // evaluation costs at most one eval + jobset + project lookup, regardless of
  // how many builds share it.
  let mut context_by_eval: std::collections::HashMap<
    Uuid,
    (Uuid, String, Uuid, String),
  > = std::collections::HashMap::new();
  for b in running.iter().chain(pending.iter()) {
    if context_by_eval.contains_key(&b.evaluation_id) {
      continue;
    }
    let Ok(eval) =
      circus_common::repo::evaluations::get(&state.pool, b.evaluation_id).await
    else {
      continue;
    };
    let Ok(jobset) =
      circus_common::repo::jobsets::get(&state.pool, eval.jobset_id).await
    else {
      continue;
    };
    let Ok(project) =
      circus_common::repo::projects::get(&state.pool, jobset.project_id).await
    else {
      continue;
    };
    context_by_eval.insert(
      b.evaluation_id,
      (project.id, project.name, jobset.id, jobset.name),
    );
  }

  let context_for = |b: &circus_common::models::Build| {
    context_by_eval.get(&b.evaluation_id).map_or_else(
      || (None, String::new(), None, String::new()),
      |(pid, pname, jid, jname)| {
        (Some(*pid), pname.clone(), Some(*jid), jname.clone())
      },
    )
  };

  let running_count = running.len() as i64;
  let pending_count = pending.len() as i64;

  // Convert running builds with elapsed time
  let running_builds: Vec<QueueBuildView> = running
    .iter()
    .map(|b| {
      let elapsed = b.started_at.map_or_else(String::new, |started| {
        let dur = chrono::Utc::now() - started;
        format_elapsed(dur.num_seconds())
      });
      let builder_name = b
        .builder_id
        .and_then(|id| builder_map.get(&id).cloned())
        .or_else(|| {
          b.agent_machine_id
            .and_then(|id| agent_map.get(&id).cloned())
        });
      let (project_id, project_name, jobset_id, jobset_name) = context_for(b);
      QueueBuildView {
        id: b.id,
        job_name: b.job_name.clone(),
        project_id,
        project_name,
        jobset_id,
        jobset_name,
        system: b.system.clone().unwrap_or_else(|| "unknown".to_string()),
        created_at: b.created_at.format("%Y-%m-%d %H:%M").to_string(),
        started_at: b
          .started_at
          .map(|t| t.format("%H:%M:%S").to_string())
          .unwrap_or_default(),
        elapsed,
        started_epoch: b.started_at.map(|t| t.timestamp()),
        priority: b.priority,
        builder_name,
        queue_pos: 0,
      }
    })
    .collect();

  // Convert pending builds with queue position
  let pending_builds: Vec<QueueBuildView> = pending
    .iter()
    .enumerate()
    .map(|(idx, b)| {
      let (project_id, project_name, jobset_id, jobset_name) = context_for(b);
      QueueBuildView {
        id: b.id,
        job_name: b.job_name.clone(),
        project_id,
        project_name,
        jobset_id,
        jobset_name,
        system: b.system.clone().unwrap_or_else(|| "unknown".to_string()),
        created_at: b.created_at.format("%Y-%m-%d %H:%M").to_string(),
        started_at: String::new(),
        elapsed: String::new(),
        started_epoch: None,
        priority: b.priority,
        builder_name: None,
        queue_pos: (idx + 1) as i64,
      }
    })
    .collect();

  let tmpl = QueueTemplate {
    pending_builds,
    running_builds,
    pending_count,
    running_count,
    permissions: UiPermissions::from_extensions(&extensions),
    csrf_token: super::csrf::csrf_from(&extensions),
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  tmpl.render_html_or_500()
}

/// Render the list of all release channels at `/channels`.
pub(super) async fn channels_page(
  State(state): State<AppState>,
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  enforce_page_access(
    &state.config.server,
    &extensions,
    DashboardPage::Channels,
  )?;
  let channels = circus_common::repo::channels::list_all(&state.pool)
    .await
    .unwrap_or_default();

  let tmpl = ChannelsTemplate {
    channels,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  tmpl.render_html_or_500()
}

pub(super) async fn channel_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  enforce_page_access(
    &state.config.server,
    &extensions,
    DashboardPage::Channel,
  )?;
  let Ok(channel) = circus_common::repo::channels::get(&state.pool, id).await
  else {
    return Ok(Html("Channel not found".to_string()));
  };

  let builds = if let Some(eval_id) = channel.current_evaluation_id {
    circus_common::repo::builds::list_for_evaluation(&state.pool, eval_id)
      .await
      .unwrap_or_default()
  } else {
    Vec::new()
  };

  let succeeded_count = builds
    .iter()
    .filter(|b| b.status == BuildStatus::Succeeded)
    .count() as i64;
  let failed_count = builds
    .iter()
    .filter(|b| {
      matches!(
        b.status,
        BuildStatus::Failed
          | BuildStatus::FailedWithOutput
          | BuildStatus::Timeout
          | BuildStatus::DependencyFailed
          | BuildStatus::Aborted
      )
    })
    .count() as i64;
  let pending_count = builds
    .iter()
    .filter(|b| matches!(b.status, BuildStatus::Pending | BuildStatus::Running))
    .count() as i64;

  let tmpl = ChannelTemplate {
    channel,
    builds: builds.iter().map(build_view).collect(),
    succeeded_count,
    failed_count,
    pending_count,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  tmpl.render_html_or_500()
}

/// Render `/starred`: the signed-in user's starred jobs with the latest
/// build status for each. Anonymous visitors see an empty page.
pub(super) async fn starred_page(
  State(state): State<AppState>,
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  enforce_page_access(
    &state.config.server,
    &extensions,
    DashboardPage::Starred,
  )?;
  // Session login (User) or API-key auth (ApiKey with user_id) both count
  // as logged in. API keys without a bound user_id can't list starred jobs.
  let user = extensions.get::<circus_common::models::User>().cloned();
  let api_key_user_id = extensions
    .get::<circus_common::models::ApiKey>()
    .and_then(|k| k.user_id);
  let viewer_user_id = user.as_ref().map(|u| u.id).or(api_key_user_id);
  let is_logged_in = viewer_user_id.is_some();

  let starred_jobs = if let Some(uid) = viewer_user_id {
    let starred = circus_common::repo::starred_jobs::list_for_user(
      &state.pool,
      uid,
      100,
      0,
    )
    .await
    .unwrap_or_default();

    let mut views = Vec::new();
    for s in starred {
      // Get project name
      let project_name =
        circus_common::repo::projects::get(&state.pool, s.project_id)
          .await
          .map_or_else(|_| "-".to_string(), |p| p.name);

      // Get jobset name
      let jobset_name = if let Some(js_id) = s.jobset_id {
        circus_common::repo::jobsets::get(&state.pool, js_id)
          .await
          .map_or_else(|_| "-".to_string(), |j| j.name)
      } else {
        "-".to_string()
      };

      // Get latest build for this job, filtered by jobset context
      let (status_text, status_class, latest_build_id) =
        if let Some(js_id) = s.jobset_id {
          // Get latest evaluation for this jobset to find relevant builds
          let evals =
            circus_common::repo::evaluations::list_filtered_with_visibility(
              &state.pool,
              Some(js_id),
              None,
              1,
              0,
              is_admin(&extensions),
            )
            .await
            .unwrap_or_default();

          let builds = if let Some(eval) = evals.first() {
            circus_common::repo::builds::list_filtered(
              &state.pool,
              Some(eval.id),
              None,
              None,
              Some(&s.job_name),
              1,
              0,
            )
            .await
            .unwrap_or_default()
          } else {
            Vec::new()
          };

          builds.first().map_or_else(
            || ("No builds".to_string(), "pending".to_string(), None),
            |build| {
              let (text, class) = status_badge(build.status);
              (text, class, Some(build.id))
            },
          )
        } else {
          ("No builds".to_string(), "pending".to_string(), None)
        };

      views.push(StarredJobView {
        id: s.id,
        project_id: s.project_id,
        project_name,
        jobset_id: s.jobset_id,
        jobset_name,
        job_name: s.job_name,
        status_text,
        status_class,
        latest_build_id,
      });
    }
    views
  } else {
    Vec::new()
  };

  let tmpl = StarredTemplate {
    starred_jobs,
    is_logged_in,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
    csrf_token: super::csrf::csrf_from(&extensions),
  };
  tmpl.render_html_or_500()
}

pub(super) async fn metrics_page(
  State(state): State<AppState>,
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  enforce_page_access(
    &state.config.server,
    &extensions,
    DashboardPage::Metrics,
  )?;
  let tmpl = MetricsTemplate {
    is_admin:  is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  tmpl.render_html_or_500()
}

pub(super) async fn project_setup_page(
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  if !is_admin(&extensions) {
    let target = if auth_name(&extensions).is_empty() {
      "/login"
    } else {
      "/projects"
    };
    return Err(Redirect::to(target).into_response());
  }

  let tmpl = ProjectSetupTemplate {
    is_admin:   is_admin(&extensions),
    auth_name:  auth_name(&extensions),
    csrf_token: super::csrf::csrf_from(&extensions),
  };
  tmpl.render_html_or_500()
}

#[cfg(test)]
mod tests {
  use super::BuildFilterParams;

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
}
