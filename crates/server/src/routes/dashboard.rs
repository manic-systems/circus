use askama::Template;
use axum::{
  Form,
  Router,
  extract::{Path, Query, State},
  http::{Extensions, StatusCode},
  response::{Html, IntoResponse, Redirect, Response},
  routing::get,
};
use fc_common::models::{
  ApiKey,
  Build,
  BuildProduct,
  BuildStatus,
  BuildStep,
  Channel,
  CreateNotificationConfig,
  Evaluation,
  EvaluationStatus,
  Jobset,
  NotificationConfig,
  Project,
  SystemStatus,
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::state::AppState;

// View models (pre-formatted for templates)

struct BuildView {
  id:            Uuid,
  job_name:      String,
  status_text:   String,
  status_class:  String,
  system:        String,
  created_at:    String,
  started_at:    String,
  completed_at:  String,
  duration:      String,
  priority:      i32,
  is_aggregate:  bool,
  signed:        bool,
  drv_path:      String,
  output_path:   String,
  error_message: String,
  log_url:       String,
}

/// Queue page build info with elapsed time and builder details
struct QueueBuildView {
  id:           Uuid,
  job_name:     String,
  system:       String,
  created_at:   String,
  started_at:   String,
  elapsed:      String,
  priority:     i32,
  builder_name: Option<String>,
  queue_pos:    i64,
}

struct EvalView {
  id:            Uuid,
  commit_hash:   String,
  commit_short:  String,
  status_text:   String,
  status_class:  String,
  time:          String,
  error_message: Option<String>,
  jobset_name:   String,
  project_name:  String,
}

struct EvalSummaryView {
  id:           Uuid,
  commit_short: String,
  status_text:  String,
  status_class: String,
  time:         String,
  succeeded:    i64,
  failed:       i64,
  pending:      i64,
}

struct ProjectSummaryView {
  id:               Uuid,
  name:             String,
  jobset_count:     i64,
  last_eval_status: String,
  last_eval_class:  String,
  last_eval_time:   String,
}

struct ApiKeyView {
  id:           Uuid,
  name:         String,
  role:         String,
  created_at:   String,
  last_used_at: String,
}

struct UserView {
  id:            Uuid,
  username:      String,
  email:         String,
  role:          String,
  user_type:     String,
  enabled:       bool,
  last_login_at: String,
}

struct StarredJobView {
  id:              Uuid,
  project_id:      Uuid,
  project_name:    String,
  jobset_id:       Option<Uuid>,
  jobset_name:     String,
  job_name:        String,
  status_text:     String,
  status_class:    String,
  latest_build_id: Option<Uuid>,
}

fn format_duration(
  started: Option<&chrono::DateTime<chrono::Utc>>,
  completed: Option<&chrono::DateTime<chrono::Utc>>,
) -> String {
  match (started, completed) {
    (Some(s), Some(c)) => {
      let secs = (*c - *s).num_seconds();
      if secs < 0 {
        return String::new();
      }
      let mins = secs / 60;
      let rem = secs % 60;
      if mins > 0 {
        format!("{mins}m {rem}s")
      } else {
        format!("{rem}s")
      }
    },
    _ => String::new(),
  }
}

fn build_view(b: &Build) -> BuildView {
  let (text, class) = status_badge(&b.status);
  BuildView {
    id:            b.id,
    job_name:      b.job_name.clone(),
    status_text:   text,
    status_class:  class,
    system:        b.system.clone().unwrap_or_else(|| "-".to_string()),
    created_at:    b.created_at.format("%Y-%m-%d %H:%M").to_string(),
    started_at:    b
      .started_at
      .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
      .unwrap_or_default(),
    completed_at:  b
      .completed_at
      .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
      .unwrap_or_default(),
    duration:      format_duration(
      b.started_at.as_ref(),
      b.completed_at.as_ref(),
    ),
    priority:      b.priority,
    is_aggregate:  b.is_aggregate,
    signed:        b.signed,
    drv_path:      b.drv_path.clone(),
    output_path:   b.build_output_path.clone().unwrap_or_default(),
    error_message: b.error_message.clone().unwrap_or_default(),
    log_url:       b.log_url.clone().unwrap_or_default(),
  }
}

fn eval_view(e: &Evaluation) -> EvalView {
  let (text, class) = eval_badge(&e.status);
  let short = if e.commit_hash.len() > 12 {
    e.commit_hash[..12].to_string()
  } else {
    e.commit_hash.clone()
  };
  EvalView {
    id:            e.id,
    commit_hash:   e.commit_hash.clone(),
    commit_short:  short,
    status_text:   text,
    status_class:  class,
    time:          e.evaluation_time.format("%Y-%m-%d %H:%M").to_string(),
    error_message: e.error_message.clone(),
    jobset_name:   String::new(),
    project_name:  String::new(),
  }
}

fn eval_view_with_context(
  e: &Evaluation,
  jobset_name: &str,
  project_name: &str,
) -> EvalView {
  let mut v = eval_view(e);
  v.jobset_name = jobset_name.to_string();
  v.project_name = project_name.to_string();
  v
}

fn status_badge(s: &BuildStatus) -> (String, String) {
  match s {
    BuildStatus::Succeeded => ("Succeeded".into(), "succeeded".into()),
    BuildStatus::Failed => ("Failed".into(), "failed".into()),
    BuildStatus::Running => ("Running".into(), "running".into()),
    BuildStatus::Pending => ("Pending".into(), "pending".into()),
    BuildStatus::Cancelled => ("Cancelled".into(), "cancelled".into()),
    BuildStatus::DependencyFailed => {
      ("Dependency Failed".into(), "failed".into())
    },
    BuildStatus::Aborted => ("Aborted".into(), "aborted".into()),
    BuildStatus::FailedWithOutput => {
      ("Failed w/ Output".into(), "failed".into())
    },
    BuildStatus::Timeout => ("Timeout".into(), "failed".into()),
    BuildStatus::CachedFailure => ("Cached Failure".into(), "failed".into()),
    BuildStatus::UnsupportedSystem => {
      ("Unsupported System".into(), "skipped".into())
    },
    BuildStatus::LogLimitExceeded => ("Log Limit".into(), "failed".into()),
    BuildStatus::NarSizeLimitExceeded => {
      ("NAR Size Limit".into(), "failed".into())
    },
    BuildStatus::NonDeterministic => {
      ("Non-deterministic".into(), "failed".into())
    },
  }
}

fn eval_badge(s: &EvaluationStatus) -> (String, String) {
  match s {
    EvaluationStatus::Completed => ("Completed".into(), "completed".into()),
    EvaluationStatus::Failed => ("Failed".into(), "failed".into()),
    EvaluationStatus::Running => ("Running".into(), "running".into()),
    EvaluationStatus::Pending => ("Pending".into(), "pending".into()),
  }
}

fn is_admin(extensions: &Extensions) -> bool {
  extensions
    .get::<ApiKey>()
    .is_some_and(|k| k.role == "admin")
}

fn auth_name(extensions: &Extensions) -> String {
  extensions
    .get::<ApiKey>()
    .map(|k| k.name.clone())
    .unwrap_or_default()
}

// Askama templates

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {
  total_builds:     i64,
  completed_builds: i64,
  failed_builds:    i64,
  running_builds:   i64,
  pending_builds:   i64,
  recent_builds:    Vec<BuildView>,
  recent_evals:     Vec<EvalView>,
  projects:         Vec<ProjectSummaryView>,
  is_admin:         bool,
  auth_name:        String,
}

#[derive(Template)]
#[template(path = "projects.html")]
struct ProjectsTemplate {
  projects:    Vec<Project>,
  limit:       i64,
  has_prev:    bool,
  has_next:    bool,
  prev_offset: i64,
  next_offset: i64,
  page:        i64,
  total_pages: i64,
  is_admin:    bool,
  auth_name:   String,
}

#[derive(Template)]
#[template(path = "project.html")]
struct ProjectTemplate {
  project:      Project,
  jobsets:      Vec<Jobset>,
  recent_evals: Vec<EvalView>,
  is_admin:     bool,
  auth_name:    String,
}

#[derive(Template)]
#[template(path = "jobset.html")]
struct JobsetTemplate {
  project:        Project,
  jobset:         Jobset,
  eval_summaries: Vec<EvalSummaryView>,
}

#[derive(Template)]
#[template(path = "evaluations.html")]
struct EvaluationsTemplate {
  evals:       Vec<EvalView>,
  limit:       i64,
  has_prev:    bool,
  has_next:    bool,
  prev_offset: i64,
  next_offset: i64,
  page:        i64,
  total_pages: i64,
}

#[derive(Template)]
#[template(path = "evaluation.html")]
struct EvaluationTemplate {
  eval:            EvalView,
  builds:          Vec<BuildView>,
  project_name:    String,
  project_id:      Uuid,
  jobset_name:     String,
  jobset_id:       Uuid,
  succeeded_count: i64,
  failed_count:    i64,
  running_count:   i64,
  pending_count:   i64,
}

#[derive(Template)]
#[template(path = "builds.html")]
struct BuildsTemplate {
  builds:        Vec<BuildView>,
  limit:         i64,
  has_prev:      bool,
  has_next:      bool,
  prev_offset:   i64,
  next_offset:   i64,
  page:          i64,
  total_pages:   i64,
  filter_status: String,
  filter_system: String,
  filter_job:    String,
}

#[derive(Template)]
#[template(path = "build.html")]
struct BuildTemplate {
  build:             BuildView,
  steps:             Vec<BuildStep>,
  products:          Vec<BuildProduct>,
  eval_id:           Uuid,
  eval_commit_short: String,
  jobset_id:         Uuid,
  jobset_name:       String,
  project_id:        Uuid,
  project_name:      String,
}

#[derive(Template)]
#[template(path = "queue.html")]
struct QueueTemplate {
  pending_builds: Vec<QueueBuildView>,
  running_builds: Vec<QueueBuildView>,
  pending_count:  i64,
  running_count:  i64,
}

#[derive(Template)]
#[template(path = "channels.html")]
struct ChannelsTemplate {
  channels: Vec<Channel>,
}

/// Builder info with load and activity metrics
struct BuilderView {
  id:             Uuid,
  name:           String,
  ssh_uri:        String,
  systems:        String,
  max_jobs:       i32,
  enabled:        bool,
  current_builds: i64,
  load_percent:   i64,
  #[allow(dead_code)]
  last_activity:  String,
}

#[derive(Template)]
#[template(path = "admin.html")]
struct AdminTemplate {
  status:    SystemStatus,
  builders:  Vec<BuilderView>,
  api_keys:  Vec<ApiKeyView>,
  is_admin:  bool,
  auth_name: String,
}

#[derive(Template)]
#[template(path = "project_setup.html")]
#[allow(dead_code)]
struct ProjectSetupTemplate {
  is_admin:  bool,
  auth_name: String,
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
  error: Option<String>,
}

#[derive(Template)]
#[template(path = "users.html")]
struct UsersTemplate {
  users:       Vec<UserView>,
  limit:       i64,
  has_prev:    bool,
  has_next:    bool,
  prev_offset: i64,
  next_offset: i64,
  page:        i64,
  total_pages: i64,
  is_admin:    bool,
  auth_name:   String,
}

#[derive(Template)]
#[template(path = "starred.html")]
struct StarredTemplate {
  starred_jobs: Vec<StarredJobView>,
  is_logged_in: bool,
  #[allow(dead_code)]
  is_admin:     bool,
  auth_name:    String,
}

#[derive(Template)]
#[template(path = "metrics.html")]
struct MetricsTemplate {
  is_admin:  bool,
  auth_name: String,
}

// Route handlers

async fn home(
  State(state): State<AppState>,
  extensions: Extensions,
) -> Html<String> {
  let build_stats = fc_common::repo::builds::get_stats(&state.pool)
    .await
    .unwrap_or_default();
  let builds = fc_common::repo::builds::list_recent(&state.pool, 10)
    .await
    .unwrap_or_default();
  let evals =
    fc_common::repo::evaluations::list_filtered(&state.pool, None, None, 5, 0)
      .await
      .unwrap_or_default();

  // Fetch project summaries
  let all_projects = fc_common::repo::projects::list(&state.pool, 10, 0)
    .await
    .unwrap_or_default();
  let mut project_summaries = Vec::new();
  for p in &all_projects {
    let jobset_count =
      fc_common::repo::jobsets::count_for_project(&state.pool, p.id)
        .await
        .unwrap_or(0);
    let jobsets =
      fc_common::repo::jobsets::list_for_project(&state.pool, p.id, 100, 0)
        .await
        .unwrap_or_default();
    let mut last_eval: Option<Evaluation> = None;
    for js in &jobsets {
      let js_evals = fc_common::repo::evaluations::list_filtered(
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

#[derive(serde::Deserialize)]
struct PageParams {
  limit:  Option<i64>,
  offset: Option<i64>,
}

async fn projects_page(
  State(state): State<AppState>,
  Query(params): Query<PageParams>,
  extensions: Extensions,
) -> Html<String> {
  let limit = params.limit.unwrap_or(50).clamp(1, 200);
  let offset = params.offset.unwrap_or(0).max(0);
  let items = fc_common::repo::projects::list(&state.pool, limit, offset)
    .await
    .unwrap_or_default();
  let total = fc_common::repo::projects::count(&state.pool)
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

async fn project_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  extensions: Extensions,
) -> Html<String> {
  let Ok(project) = fc_common::repo::projects::get(&state.pool, id).await
  else {
    return Html("Project not found".to_string());
  };
  let jobsets =
    fc_common::repo::jobsets::list_for_project(&state.pool, id, 100, 0)
      .await
      .unwrap_or_default();

  // Get evaluations for this project's jobsets
  let mut evals = Vec::new();
  for js in &jobsets {
    let mut js_evals = fc_common::repo::evaluations::list_filtered(
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

async fn jobset_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Html<String> {
  let Ok(jobset) = fc_common::repo::jobsets::get(&state.pool, id).await else {
    return Html("Jobset not found".to_string());
  };
  let Ok(project) =
    fc_common::repo::projects::get(&state.pool, jobset.project_id).await
  else {
    return Html("Project not found".to_string());
  };

  let evals = fc_common::repo::evaluations::list_filtered(
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
    let succeeded = fc_common::repo::builds::count_filtered(
      &state.pool,
      Some(e.id),
      Some("completed"),
      None,
      None,
    )
    .await
    .unwrap_or(0);
    let failed = fc_common::repo::builds::count_filtered(
      &state.pool,
      Some(e.id),
      Some("failed"),
      None,
      None,
    )
    .await
    .unwrap_or(0);
    let pending = fc_common::repo::builds::count_filtered(
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
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

async fn evaluations_page(
  State(state): State<AppState>,
  Query(params): Query<PageParams>,
) -> Html<String> {
  let limit = params.limit.unwrap_or(50).clamp(1, 200);
  let offset = params.offset.unwrap_or(0).max(0);
  let items = fc_common::repo::evaluations::list_filtered(
    &state.pool,
    None,
    None,
    limit,
    offset,
  )
  .await
  .unwrap_or_default();
  let total =
    fc_common::repo::evaluations::count_filtered(&state.pool, None, None)
      .await
      .unwrap_or(0);

  // Enrich evaluations with jobset/project names
  let mut enriched = Vec::new();
  for e in &items {
    let (jname, pname) =
      match fc_common::repo::jobsets::get(&state.pool, e.jobset_id).await {
        Ok(js) => {
          let pname =
            fc_common::repo::projects::get(&state.pool, js.project_id)
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
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

async fn evaluation_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Html<String> {
  let Ok(eval) = fc_common::repo::evaluations::get(&state.pool, id).await
  else {
    return Html("Evaluation not found".to_string());
  };

  let Ok(jobset) =
    fc_common::repo::jobsets::get(&state.pool, eval.jobset_id).await
  else {
    return Html("Jobset not found".to_string());
  };
  let Ok(project) =
    fc_common::repo::projects::get(&state.pool, jobset.project_id).await
  else {
    return Html("Project not found".to_string());
  };

  let builds = fc_common::repo::builds::list_filtered(
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

  let succeeded = fc_common::repo::builds::count_filtered(
    &state.pool,
    Some(id),
    Some("completed"),
    None,
    None,
  )
  .await
  .unwrap_or(0);
  let failed = fc_common::repo::builds::count_filtered(
    &state.pool,
    Some(id),
    Some("failed"),
    None,
    None,
  )
  .await
  .unwrap_or(0);
  let running = fc_common::repo::builds::count_filtered(
    &state.pool,
    Some(id),
    Some("running"),
    None,
    None,
  )
  .await
  .unwrap_or(0);
  let pending = fc_common::repo::builds::count_filtered(
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
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

#[derive(serde::Deserialize)]
struct BuildFilterParams {
  status:   Option<String>,
  system:   Option<String>,
  job_name: Option<String>,
  limit:    Option<i64>,
  offset:   Option<i64>,
}

async fn builds_page(
  State(state): State<AppState>,
  Query(params): Query<BuildFilterParams>,
) -> Html<String> {
  let limit = params.limit.unwrap_or(50).clamp(1, 200);
  let offset = params.offset.unwrap_or(0).max(0);
  let items = fc_common::repo::builds::list_filtered(
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
  let total = fc_common::repo::builds::count_filtered(
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
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

async fn build_page(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Html<String> {
  let Ok(build) = fc_common::repo::builds::get(&state.pool, id).await else {
    return Html("Build not found".to_string());
  };

  let Ok(eval) =
    fc_common::repo::evaluations::get(&state.pool, build.evaluation_id).await
  else {
    return Html("Evaluation not found".to_string());
  };
  let Ok(jobset) =
    fc_common::repo::jobsets::get(&state.pool, eval.jobset_id).await
  else {
    return Html("Jobset not found".to_string());
  };
  let Ok(project) =
    fc_common::repo::projects::get(&state.pool, jobset.project_id).await
  else {
    return Html("Project not found".to_string());
  };

  let eval_commit_short = if eval.commit_hash.len() > 12 {
    eval.commit_hash[..12].to_string()
  } else {
    eval.commit_hash.clone()
  };

  let steps = fc_common::repo::build_steps::list_for_build(&state.pool, id)
    .await
    .unwrap_or_default();
  let products =
    fc_common::repo::build_products::list_for_build(&state.pool, id)
      .await
      .unwrap_or_default();

  let tmpl = BuildTemplate {
    build: build_view(&build),
    steps,
    products,
    eval_id: eval.id,
    eval_commit_short,
    jobset_id: jobset.id,
    jobset_name: jobset.name,
    project_id: project.id,
    project_name: project.name,
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

async fn queue_page(State(state): State<AppState>) -> Html<String> {
  let running = fc_common::repo::builds::list_filtered(
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
  let pending = fc_common::repo::builds::list_filtered(
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
  let builders = fc_common::repo::remote_builders::list(&state.pool)
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
        id:           b.id,
        job_name:     b.job_name.clone(),
        system:       b.system.clone().unwrap_or_else(|| "unknown".to_string()),
        created_at:   b.created_at.format("%Y-%m-%d %H:%M").to_string(),
        started_at:   String::new(),
        elapsed:      String::new(),
        priority:     b.priority,
        builder_name: None,
        queue_pos:    (idx + 1) as i64,
      }
    })
    .collect();

  let tmpl = QueueTemplate {
    pending_builds,
    running_builds,
    pending_count,
    running_count,
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

fn format_elapsed(secs: i64) -> String {
  if secs < 60 {
    format!("{secs}s")
  } else if secs < 3600 {
    format!("{}m {}s", secs / 60, secs % 60)
  } else {
    format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
  }
}

async fn channels_page(State(state): State<AppState>) -> Html<String> {
  let channels = fc_common::repo::channels::list_all(&state.pool)
    .await
    .unwrap_or_default();

  let tmpl = ChannelsTemplate { channels };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

async fn admin_page(
  State(state): State<AppState>,
  extensions: Extensions,
) -> Html<String> {
  let pool = &state.pool;

  let projects: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects")
    .fetch_one(pool)
    .await
    .unwrap_or((0,));
  let jobsets: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobsets")
    .fetch_one(pool)
    .await
    .unwrap_or((0,));
  let evaluations: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM evaluations")
    .fetch_one(pool)
    .await
    .unwrap_or((0,));
  let build_stats = fc_common::repo::builds::get_stats(pool)
    .await
    .unwrap_or_default();
  let builders_count = fc_common::repo::remote_builders::count(pool)
    .await
    .unwrap_or(0);
  let channels: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM channels")
    .fetch_one(pool)
    .await
    .unwrap_or((0,));

  let status = SystemStatus {
    projects_count:    projects.0,
    jobsets_count:     jobsets.0,
    evaluations_count: evaluations.0,
    builds_pending:    build_stats.pending_builds.unwrap_or(0),
    builds_running:    build_stats.running_builds.unwrap_or(0),
    builds_completed:  build_stats.completed_builds.unwrap_or(0),
    builds_failed:     build_stats.failed_builds.unwrap_or(0),
    remote_builders:   builders_count,
    channels_count:    channels.0,
  };
  let raw_builders = fc_common::repo::remote_builders::list(pool)
    .await
    .unwrap_or_default();

  // Get running builds to calculate builder load
  let running_builds = fc_common::repo::builds::list_filtered(
    pool,
    None,
    Some("running"),
    None,
    None,
    1000,
    0,
  )
  .await
  .unwrap_or_default();

  // Count builds per builder
  let mut builds_per_builder: std::collections::HashMap<Uuid, i64> =
    std::collections::HashMap::new();
  for build in &running_builds {
    if let Some(builder_id) = build.builder_id {
      *builds_per_builder.entry(builder_id).or_insert(0) += 1;
    }
  }

  // Convert to BuilderView with load info
  let builders: Vec<BuilderView> = raw_builders
    .into_iter()
    .map(|b| {
      let current_builds = *builds_per_builder.get(&b.id).unwrap_or(&0);
      let load_percent = if b.max_jobs > 0 {
        (current_builds * 100) / i64::from(b.max_jobs)
      } else {
        0
      };
      BuilderView {
        id: b.id,
        name: b.name,
        ssh_uri: b.ssh_uri,
        systems: b.systems.join(", "),
        max_jobs: b.max_jobs,
        enabled: b.enabled,
        current_builds,
        load_percent,
        last_activity: b.created_at.format("%Y-%m-%d").to_string(),
      }
    })
    .collect();

  // Fetch API keys for admin view
  let keys = fc_common::repo::api_keys::list(pool)
    .await
    .unwrap_or_default();
  let api_keys: Vec<ApiKeyView> = keys
    .into_iter()
    .map(|k| {
      ApiKeyView {
        id:           k.id,
        name:         k.name,
        role:         k.role,
        created_at:   k.created_at.format("%Y-%m-%d %H:%M").to_string(),
        last_used_at: k.last_used_at.map_or_else(
          || "Never".to_string(),
          |t| t.format("%Y-%m-%d %H:%M").to_string(),
        ),
      }
    })
    .collect();

  let tmpl = AdminTemplate {
    status,
    builders,
    api_keys,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
  };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

// Setup wizard

async fn project_setup_page(extensions: Extensions) -> Html<String> {
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

// Login / Logout

async fn login_page() -> Html<String> {
  let tmpl = LoginTemplate { error: None };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

#[derive(serde::Deserialize)]
struct LoginForm {
  username: Option<String>,
  api_key:  Option<String>,
  password: Option<String>,
}

async fn login_action(
  State(state): State<AppState>,
  Form(form): Form<LoginForm>,
) -> Response {
  // Try username/password authentication first
  if let (Some(username), Some(password)) =
    (form.username.as_ref(), form.password.as_ref())
  {
    let creds = fc_common::models::LoginCredentials {
      username: username.clone(),
      password: password.clone(),
    };

    if let Ok(user) =
      fc_common::repo::users::authenticate(&state.pool, &creds).await
    {
      let session_id = Uuid::new_v4().to_string();
      state
        .sessions
        .insert(session_id.clone(), crate::state::SessionData {
          api_key:    None,
          user:       Some(user),
          created_at: std::time::Instant::now(),
        });

      let security_flags =
        crate::routes::cookie_security_flags(&state.config.server);
      let cookie = format!(
        "fc_user_session={session_id}; {security_flags}; Path=/; Max-Age=86400"
      );
      return (
        [(axum::http::header::SET_COOKIE, cookie)],
        Redirect::to("/"),
      )
        .into_response();
    } else {
      let tmpl = LoginTemplate {
        error: Some("Invalid username or password".to_string()),
      };
      return (
        StatusCode::UNAUTHORIZED,
        Html(
          tmpl
            .render()
            .unwrap_or_else(|e| format!("Template error: {e}")),
        ),
      )
        .into_response();
    }
  }

  // Fall back to API key authentication
  if let Some(token) = form.api_key.as_ref() {
    let token = token.trim();
    if token.is_empty() {
      let tmpl = LoginTemplate {
        error: Some("API key is required".to_string()),
      };
      return Html(
        tmpl
          .render()
          .unwrap_or_else(|e| format!("Template error: {e}")),
      )
      .into_response();
    }

    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let key_hash = hex::encode(hasher.finalize());

    if let Ok(Some(api_key)) =
      fc_common::repo::api_keys::get_by_hash(&state.pool, &key_hash).await
    {
      let session_id = Uuid::new_v4().to_string();
      state
        .sessions
        .insert(session_id.clone(), crate::state::SessionData {
          api_key:    Some(api_key),
          user:       None,
          created_at: std::time::Instant::now(),
        });

      let security_flags =
        crate::routes::cookie_security_flags(&state.config.server);
      let cookie = format!(
        "fc_session={session_id}; {security_flags}; Path=/; Max-Age=86400"
      );
      (
        [(axum::http::header::SET_COOKIE, cookie)],
        Redirect::to("/"),
      )
        .into_response()
    } else {
      let tmpl = LoginTemplate {
        error: Some("Invalid API key".to_string()),
      };
      Html(
        tmpl
          .render()
          .unwrap_or_else(|e| format!("Template error: {e}")),
      )
      .into_response()
    }
  } else {
    let tmpl = LoginTemplate {
      error: Some(
        "Please provide either username/password or API key".to_string(),
      ),
    };
    Html(
      tmpl
        .render()
        .unwrap_or_else(|e| format!("Template error: {e}")),
    )
    .into_response()
  }
}

async fn logout_action(
  State(state): State<AppState>,
  request: axum::extract::Request,
) -> Response {
  // Remove server-side session for both cookie types
  if let Some(cookie_header) = request
    .headers()
    .get("cookie")
    .and_then(|v| v.to_str().ok())
  {
    // Check for user session
    if let Some(session_id) = cookie_header.split(';').find_map(|pair| {
      let pair = pair.trim();
      let (k, v) = pair.split_once('=')?;
      if k.trim() == "fc_user_session" {
        Some(v.trim().to_string())
      } else {
        None
      }
    }) {
      state.sessions.remove(&session_id);
    }

    // Check for legacy API key session
    if let Some(session_id) = cookie_header.split(';').find_map(|pair| {
      let pair = pair.trim();
      let (k, v) = pair.split_once('=')?;
      if k.trim() == "fc_session" {
        Some(v.trim().to_string())
      } else {
        None
      }
    }) {
      state.sessions.remove(&session_id);
    }
  }

  // Clear both cookies
  let cookies = [
    "fc_user_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
    "fc_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
  ];
  (
    [
      (axum::http::header::SET_COOKIE, cookies[0].to_string()),
      (axum::http::header::SET_COOKIE, cookies[1].to_string()),
    ],
    Redirect::to("/"),
  )
    .into_response()
}

async fn users_page(
  State(state): State<AppState>,
  Query(params): Query<PageParams>,
  extensions: Extensions,
) -> Result<Html<String>, axum::response::Response> {
  // Only admins can view user list (contains PII like emails)
  if !is_admin(&extensions) {
    return Err(axum::response::Redirect::to("/").into_response());
  }

  let limit = params.limit.unwrap_or(50).clamp(1, 200);
  let offset = params.offset.unwrap_or(0).max(0);

  let users_list = fc_common::repo::users::list(&state.pool, limit, offset)
    .await
    .unwrap_or_default();
  let total = fc_common::repo::users::count(&state.pool)
    .await
    .unwrap_or(0);

  let users: Vec<UserView> = users_list
    .into_iter()
    .map(|u| {
      let user_type = match u.user_type {
        fc_common::models::UserType::Local => "Local",
        fc_common::models::UserType::Github => "GitHub",
        fc_common::models::UserType::Google => "Google",
        fc_common::models::UserType::Ldap => "LDAP",
      };
      UserView {
        id:            u.id,
        username:      u.username,
        email:         u.email,
        role:          u.role,
        user_type:     user_type.to_string(),
        enabled:       u.enabled,
        last_login_at: u.last_login_at.map_or_else(
          || "Never".to_string(),
          |t| t.format("%Y-%m-%d %H:%M").to_string(),
        ),
      }
    })
    .collect();

  let total_pages = (total + limit - 1) / limit.max(1);
  let page = offset / limit.max(1) + 1;

  let tmpl = UsersTemplate {
    users,
    limit,
    has_prev: offset > 0,
    has_next: offset + limit < total,
    prev_offset: (offset - limit).max(0),
    next_offset: offset + limit,
    page,
    total_pages,
    is_admin: true, // Already checked above
    auth_name: auth_name(&extensions),
  };
  Ok(Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  ))
}

async fn starred_page(
  State(state): State<AppState>,
  extensions: Extensions,
) -> Html<String> {
  // Check if user is logged in via session
  let user = extensions.get::<fc_common::models::User>().cloned();
  let is_logged_in = user.is_some();

  let starred_jobs = if let Some(ref u) = user {
    let starred =
      fc_common::repo::starred_jobs::list_for_user(&state.pool, u.id, 100, 0)
        .await
        .unwrap_or_default();

    let mut views = Vec::new();
    for s in starred {
      // Get project name
      let project_name =
        fc_common::repo::projects::get(&state.pool, s.project_id)
          .await
          .map_or_else(|_| "-".to_string(), |p| p.name);

      // Get jobset name
      let jobset_name = if let Some(js_id) = s.jobset_id {
        fc_common::repo::jobsets::get(&state.pool, js_id)
          .await
          .map_or_else(|_| "-".to_string(), |j| j.name)
      } else {
        "-".to_string()
      };

      // Get latest build for this job, filtered by jobset context
      let (status_text, status_class, latest_build_id) =
        if let Some(js_id) = s.jobset_id {
          // Get latest evaluation for this jobset to find relevant builds
          let evals = fc_common::repo::evaluations::list_filtered(
            &state.pool,
            Some(js_id),
            None,
            1,
            0,
          )
          .await
          .unwrap_or_default();

          let builds = if let Some(eval) = evals.first() {
            fc_common::repo::builds::list_filtered(
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
              let (text, class) = status_badge(&build.status);
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

async fn metrics_page(extensions: Extensions) -> Html<String> {
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

#[derive(Template)]
#[template(path = "notifications.html")]
struct NotificationsTemplate {
  project:    Project,
  configs:    Vec<NotificationConfig>,
  is_admin:   bool,
  auth_name:  String,
  csrf_token: String,
}

fn csrf_from(extensions: &Extensions) -> String {
  extensions
    .get::<crate::state::CsrfToken>()
    .map(|t| t.0.clone())
    .unwrap_or_default()
}

#[allow(clippy::result_large_err)]
fn check_csrf(
  extensions: &Extensions,
  submitted: &str,
) -> Result<(), Response> {
  use subtle::ConstantTimeEq;
  let expected = csrf_from(extensions);
  if expected.is_empty()
    || expected.as_bytes().ct_eq(submitted.as_bytes()).unwrap_u8() != 1
  {
    return Err(
      (StatusCode::FORBIDDEN, "Invalid or missing CSRF token").into_response(),
    );
  }
  Ok(())
}

#[derive(serde::Deserialize)]
pub struct NotificationCreateForm {
  pub notification_type: String,
  pub config:            String,
  pub csrf_token:        String,
}

#[derive(serde::Deserialize)]
pub struct CsrfOnlyForm {
  pub csrf_token: String,
}

async fn notifications_page(
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  let project = fc_common::repo::projects::get(&state.pool, project_id)
    .await
    .map_err(|_| Redirect::to("/projects").into_response())?;
  let configs = fc_common::repo::notification_configs::list_for_project(
    &state.pool,
    project_id,
  )
  .await
  .unwrap_or_default();
  let tmpl = NotificationsTemplate {
    project,
    configs,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
    csrf_token: csrf_from(&extensions),
  };
  Ok(Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  ))
}

async fn notifications_create(
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  extensions: Extensions,
  Form(form): Form<NotificationCreateForm>,
) -> Result<Redirect, Response> {
  if !is_admin(&extensions) {
    return Err((StatusCode::FORBIDDEN, "Admin required").into_response());
  }
  check_csrf(&extensions, &form.csrf_token)?;
  let parsed: serde_json::Value = serde_json::from_str(form.config.trim())
    .map_err(|e| {
      (StatusCode::BAD_REQUEST, format!("Invalid JSON: {e}")).into_response()
    })?;
  if !parsed.is_object() {
    return Err(
      (StatusCode::BAD_REQUEST, "Config must be a JSON object").into_response(),
    );
  }
  let allowed_types = [
    "webhook",
    "github_status",
    "gitea_status",
    "gitlab_status",
    "email",
    "slack",
  ];
  if !allowed_types.contains(&form.notification_type.as_str()) {
    return Err(
      (StatusCode::BAD_REQUEST, "Unknown notification type").into_response(),
    );
  }

  // SSRF guard: outbound webhooks must point at public HTTP(S) hosts.
  let url_field = match form.notification_type.as_str() {
    "webhook" => Some("url"),
    "slack" => Some("webhook_url"),
    _ => None,
  };
  if let Some(field) = url_field {
    let url_str =
      parsed.get(field).and_then(|v| v.as_str()).ok_or_else(|| {
        (
          StatusCode::BAD_REQUEST,
          format!("Missing '{field}' in config"),
        )
          .into_response()
      })?;
    fc_common::validate::validate_webhook_url(url_str).map_err(|e| {
      (StatusCode::BAD_REQUEST, format!("Invalid URL: {e}")).into_response()
    })?;
  }

  fc_common::repo::notification_configs::create(
    &state.pool,
    CreateNotificationConfig {
      project_id,
      notification_type: form.notification_type,
      config: parsed,
    },
  )
  .await
  .map_err(|e| {
    (StatusCode::BAD_REQUEST, format!("Create failed: {e}")).into_response()
  })?;

  Ok(Redirect::to(&format!(
    "/project/{project_id}/notifications"
  )))
}

async fn notifications_delete(
  State(state): State<AppState>,
  Path((project_id, config_id)): Path<(Uuid, Uuid)>,
  extensions: Extensions,
  Form(form): Form<CsrfOnlyForm>,
) -> Result<Redirect, Response> {
  if !is_admin(&extensions) {
    return Err((StatusCode::FORBIDDEN, "Admin required").into_response());
  }
  check_csrf(&extensions, &form.csrf_token)?;
  fc_common::repo::notification_configs::delete_for_project(
    &state.pool,
    project_id,
    config_id,
  )
  .await
  .map_err(|e| {
    (StatusCode::NOT_FOUND, format!("Delete failed: {e}")).into_response()
  })?;
  Ok(Redirect::to(&format!(
    "/project/{project_id}/notifications"
  )))
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/login", get(login_page).post(login_action))
    .route("/logout", axum::routing::post(logout_action))
    .route("/", get(home))
    .route("/projects", get(projects_page))
    .route("/projects/new", get(project_setup_page))
    .route("/project/{id}", get(project_page))
    .route(
      "/project/{id}/notifications",
      get(notifications_page).post(notifications_create),
    )
    .route(
      "/project/{id}/notifications/{config_id}/delete",
      axum::routing::post(notifications_delete),
    )
    .route("/jobset/{id}", get(jobset_page))
    .route("/evaluations", get(evaluations_page))
    .route("/evaluation/{id}", get(evaluation_page))
    .route("/builds", get(builds_page))
    .route("/build/{id}", get(build_page))
    .route("/queue", get(queue_page))
    .route("/channels", get(channels_page))
    .route("/admin", get(admin_page))
    .route("/users", get(users_page))
    .route("/starred", get(starred_page))
    .route("/metrics", get(metrics_page))
}
