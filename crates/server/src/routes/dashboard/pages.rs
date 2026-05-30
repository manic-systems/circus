//! Read-only viewing pages: home, projects, project detail, jobset detail,
//! evaluations, evaluation detail, builds, build detail, queue, channels,
//! channel detail, starred, metrics, and the project-setup wizard.
//!
//! These handlers do not mutate server state; they only render templates.
//! Mutating admin actions live in `super::admin`.

use askama::Template;
use axum::{
  extract::{Path, Query, State},
  http::Extensions,
  response::Html,
};
use circus_common::models::{BuildStatus, Evaluation};
use uuid::Uuid;

use super::{
  shared::{
    EvalSummaryView,
    ProjectSummaryView,
    QueueBuildView,
    StarredJobView,
    auth_name,
    build_view,
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
    JobsetTemplate,
    MetricsTemplate,
    ProjectSetupTemplate,
    ProjectTemplate,
    ProjectsTemplate,
    QueueTemplate,
    StarredTemplate,
  },
};
use crate::state::AppState;

#[derive(serde::Deserialize)]
pub(super) struct PageParams {
  pub(super) limit:  Option<i64>,
  pub(super) offset: Option<i64>,
}

#[derive(serde::Deserialize)]
pub(super) struct BuildFilterParams {
  status:   Option<String>,
  system:   Option<String>,
  job_name: Option<String>,
  limit:    Option<i64>,
  offset:   Option<i64>,
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

// ---------- Home ----------

pub(super) async fn home(
  State(state): State<AppState>,
  extensions: Extensions,
) -> Html<String> {
  let build_stats = circus_common::repo::builds::get_stats(&state.pool)
    .await
    .unwrap_or_default();
  let builds = circus_common::repo::builds::list_recent(&state.pool, 10)
    .await
    .unwrap_or_default();
  let evals = circus_common::repo::evaluations::list_filtered(
    &state.pool,
    None,
    None,
    5,
    0,
  )
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
      let js_evals = circus_common::repo::evaluations::list_filtered(
        &state.pool,
        Some(js.id),
        None,
        1,
        0,
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
    total_builds:     build_stats.total_builds.unwrap_or(0),
    completed_builds: build_stats.completed_builds.unwrap_or(0),
    failed_builds:    build_stats.failed_builds.unwrap_or(0),
    running_builds:   build_stats.running_builds.unwrap_or(0),
    pending_builds:   build_stats.pending_builds.unwrap_or(0),
    recent_builds:    builds.iter().map(build_view).collect(),
    recent_evals:     evals.iter().map(eval_view).collect(),
    projects:         project_summaries,
    is_admin:         is_admin(&extensions),
    auth_name:        auth_name(&extensions),
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

// ---------- Projects / Project ----------

pub(super) async fn projects_page(
  State(state): State<AppState>,
  Query(params): Query<PageParams>,
  extensions: Extensions,
) -> Html<String> {
  let limit = params.limit.unwrap_or(50).clamp(1, 200);
  let offset = params.offset.unwrap_or(0).max(0);
  let items = circus_common::repo::projects::list(&state.pool, limit, offset)
    .await
    .unwrap_or_default();
  let total = circus_common::repo::projects::count(&state.pool)
    .await
    .unwrap_or(0);

  let total_pages = (total + limit - 1) / limit.max(1);
  let page = offset / limit.max(1) + 1;
  let tmpl = ProjectsTemplate {
    projects: items,
    limit,
    has_prev: offset > 0,
    has_next: offset + limit < total,
    prev_offset: (offset - limit).max(0),
    next_offset: offset + limit,
    page,
    total_pages,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

pub(super) async fn project_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  extensions: Extensions,
) -> Html<String> {
  let Ok(project) = circus_common::repo::projects::get(&state.pool, id).await
  else {
    return Html("Project not found".to_string());
  };
  let jobsets =
    circus_common::repo::jobsets::list_for_project(&state.pool, id, 100, 0)
      .await
      .unwrap_or_default();

  // Get evaluations for this project's jobsets
  let mut evals = Vec::new();
  for js in &jobsets {
    let mut js_evals = circus_common::repo::evaluations::list_filtered(
      &state.pool,
      Some(js.id),
      None,
      5,
      0,
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
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

pub(super) async fn jobset_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  extensions: Extensions,
) -> Html<String> {
  let Ok(jobset) = circus_common::repo::jobsets::get(&state.pool, id).await
  else {
    return Html("Jobset not found".to_string());
  };
  let Ok(project) =
    circus_common::repo::projects::get(&state.pool, jobset.project_id).await
  else {
    return Html("Project not found".to_string());
  };

  let evals = circus_common::repo::evaluations::list_filtered(
    &state.pool,
    Some(id),
    None,
    20,
    0,
  )
  .await
  .unwrap_or_default();

  let mut summaries = Vec::new();
  for e in &evals {
    let (text, class) = eval_badge(&e.status);
    let short = if e.commit_hash.len() > 12 {
      e.commit_hash[..12].to_string()
    } else {
      e.commit_hash.clone()
    };
    let succeeded = circus_common::repo::builds::count_filtered(
      &state.pool,
      Some(e.id),
      Some("completed"),
      None,
      None,
    )
    .await
    .unwrap_or(0);
    let failed = circus_common::repo::builds::count_filtered(
      &state.pool,
      Some(e.id),
      Some("failed"),
      None,
      None,
    )
    .await
    .unwrap_or(0);
    let pending = circus_common::repo::builds::count_filtered(
      &state.pool,
      Some(e.id),
      Some("pending"),
      None,
      None,
    )
    .await
    .unwrap_or(0);

    summaries.push(EvalSummaryView {
      id: e.id,
      commit_short: short,
      status_text: text,
      status_class: class,
      time: e.evaluation_time.format("%Y-%m-%d %H:%M").to_string(),
      succeeded,
      failed,
      pending,
    });
  }

  let tmpl = JobsetTemplate {
    project,
    jobset,
    eval_summaries: summaries,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

// ---------- Evaluations / Evaluation ----------

pub(super) async fn evaluations_page(
  State(state): State<AppState>,
  Query(params): Query<PageParams>,
  extensions: Extensions,
) -> Html<String> {
  let limit = params.limit.unwrap_or(50).clamp(1, 200);
  let offset = params.offset.unwrap_or(0).max(0);
  let items = circus_common::repo::evaluations::list_filtered(
    &state.pool,
    None,
    None,
    limit,
    offset,
  )
  .await
  .unwrap_or_default();
  let total =
    circus_common::repo::evaluations::count_filtered(&state.pool, None, None)
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

  let total_pages = (total + limit - 1) / limit.max(1);
  let page = offset / limit.max(1) + 1;
  let tmpl = EvaluationsTemplate {
    evals: enriched,
    limit,
    has_prev: offset > 0,
    has_next: offset + limit < total,
    prev_offset: (offset - limit).max(0),
    next_offset: offset + limit,
    page,
    total_pages,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

pub(super) async fn evaluation_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  extensions: Extensions,
) -> Html<String> {
  let Ok(eval) = circus_common::repo::evaluations::get(&state.pool, id).await
  else {
    return Html("Evaluation not found".to_string());
  };

  let Ok(jobset) =
    circus_common::repo::jobsets::get(&state.pool, eval.jobset_id).await
  else {
    return Html("Jobset not found".to_string());
  };
  let Ok(project) =
    circus_common::repo::projects::get(&state.pool, jobset.project_id).await
  else {
    return Html("Project not found".to_string());
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

  let succeeded = circus_common::repo::builds::count_filtered(
    &state.pool,
    Some(id),
    Some("completed"),
    None,
    None,
  )
  .await
  .unwrap_or(0);
  let failed = circus_common::repo::builds::count_filtered(
    &state.pool,
    Some(id),
    Some("failed"),
    None,
    None,
  )
  .await
  .unwrap_or(0);
  let running = circus_common::repo::builds::count_filtered(
    &state.pool,
    Some(id),
    Some("running"),
    None,
    None,
  )
  .await
  .unwrap_or(0);
  let pending = circus_common::repo::builds::count_filtered(
    &state.pool,
    Some(id),
    Some("pending"),
    None,
    None,
  )
  .await
  .unwrap_or(0);

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
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

// ---------- Builds / Build ----------

pub(super) async fn builds_page(
  State(state): State<AppState>,
  Query(params): Query<BuildFilterParams>,
  extensions: Extensions,
) -> Html<String> {
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

  let total_pages = (total + limit - 1) / limit.max(1);
  let page = offset / limit.max(1) + 1;
  let tmpl = BuildsTemplate {
    builds: items.iter().map(build_view).collect(),
    limit,
    has_prev: offset > 0,
    has_next: offset + limit < total,
    prev_offset: (offset - limit).max(0),
    next_offset: offset + limit,
    page,
    total_pages,
    filter_status: params.status.unwrap_or_default(),
    filter_system: params.system.unwrap_or_default(),
    filter_job: params.job_name.unwrap_or_default(),
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

pub(super) async fn build_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  extensions: Extensions,
) -> Html<String> {
  let Ok(build) = circus_common::repo::builds::get(&state.pool, id).await
  else {
    return Html("Build not found".to_string());
  };

  let Ok(eval) =
    circus_common::repo::evaluations::get(&state.pool, build.evaluation_id)
      .await
  else {
    return Html("Evaluation not found".to_string());
  };
  let Ok(jobset) =
    circus_common::repo::jobsets::get(&state.pool, eval.jobset_id).await
  else {
    return Html("Jobset not found".to_string());
  };
  let Ok(project) =
    circus_common::repo::projects::get(&state.pool, jobset.project_id).await
  else {
    return Html("Project not found".to_string());
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

  let tmpl = BuildTemplate {
    build: build_view(&build),
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
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

// ---------- Queue ----------

pub(super) async fn queue_page(
  State(state): State<AppState>,
  extensions: Extensions,
) -> Html<String> {
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
  let pending = circus_common::repo::builds::list_filtered(
    &state.pool,
    None,
    Some("pending"),
    None,
    None,
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
      let builder_name =
        b.builder_id.and_then(|id| builder_map.get(&id).cloned());
      QueueBuildView {
        id: b.id,
        job_name: b.job_name.clone(),
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
      QueueBuildView {
        id:            b.id,
        job_name:      b.job_name.clone(),
        system:        b
          .system
          .clone()
          .unwrap_or_else(|| "unknown".to_string()),
        created_at:    b.created_at.format("%Y-%m-%d %H:%M").to_string(),
        started_at:    String::new(),
        elapsed:       String::new(),
        started_epoch: None,
        priority:      b.priority,
        builder_name:  None,
        queue_pos:     (idx + 1) as i64,
      }
    })
    .collect();

  let tmpl = QueueTemplate {
    pending_builds,
    running_builds,
    pending_count,
    running_count,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

// ---------- Channels ----------

pub(super) async fn channels_page(
  State(state): State<AppState>,
  extensions: Extensions,
) -> Html<String> {
  let channels = circus_common::repo::channels::list_all(&state.pool)
    .await
    .unwrap_or_default();

  let tmpl = ChannelsTemplate {
    channels,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

pub(super) async fn channel_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  extensions: Extensions,
) -> Html<String> {
  let Ok(channel) = circus_common::repo::channels::get(&state.pool, id).await
  else {
    return Html("Channel not found".to_string());
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
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

// ---------- Starred / Metrics / Setup wizard ----------

pub(super) async fn starred_page(
  State(state): State<AppState>,
  extensions: Extensions,
) -> Html<String> {
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
          let evals = circus_common::repo::evaluations::list_filtered(
            &state.pool,
            Some(js_id),
            None,
            1,
            0,
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
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

pub(super) async fn metrics_page(extensions: Extensions) -> Html<String> {
  let tmpl = MetricsTemplate {
    is_admin:  is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

pub(super) async fn project_setup_page(extensions: Extensions) -> Html<String> {
  let tmpl = ProjectSetupTemplate {
    is_admin:  is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}
