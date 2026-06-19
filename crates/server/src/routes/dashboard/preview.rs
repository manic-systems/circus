//! Fixture-backed dashboard preview routes for `cargo xtask preview-frontend`.

use std::path::PathBuf;

use askama::Template;
use axum::{
  Json,
  Router,
  body::Body,
  http::{StatusCode, header},
  response::{Html, IntoResponse, Redirect, Response},
  routing::get,
};
use chrono::{Duration, Utc};
use circus_common::models::{
  BinaryCacheUpstreams,
  BuildProduct,
  BuildStep,
  Channel,
  Jobset,
  JobsetState,
  JobsetTriggerMode,
  NewsItem,
  Project,
  SystemStatus,
};
use circus_config::UiConfig;
use sqlx::types::Json as SqlxJson;
use tower_http::services::ServeDir;
use uuid::Uuid;

use super::{
  shared::{
    ApiKeyView,
    BuildErrorLine,
    BuildView,
    EvalSummaryView,
    EvalView,
    JobStatusCell,
    JobStatusColumn,
    JobStatusRow,
    PrivateTemplate,
    ProjectSummaryView,
    QueueBuildView,
    QueueSystemView,
    StarredJobView,
    UserView,
    WorkerSummaryView,
  },
  templates::{
    AdminTemplate,
    AgentView,
    BuildTemplate,
    BuilderView,
    BuildsTemplate,
    ChannelTemplate,
    ChannelView,
    ChannelsTemplate,
    EvaluationTemplate,
    EvaluationsTemplate,
    HomeTemplate,
    JobsetJobsTemplate,
    JobsetTemplate,
    LoginTemplate,
    MetricsTemplate,
    NewsTemplate,
    NotificationTaskView,
    PinnedOutputView,
    ProjectSetupTemplate,
    ProjectTemplate,
    ProjectsTemplate,
    QueueTemplate,
    SortHeaderView,
    StarredTemplate,
    UiTemplateConfig,
    UsersTemplate,
  },
};
use crate::permissions::UiPermissions;

pub fn router() -> Router {
  Router::new()
    .route("/__preview", get(index))
    .route("/static/theme.css", get(theme_css))
    .nest_service("/static", ServeDir::new(static_dir()))
    .route("/api/v1/projects", get(api_projects))
    .route("/api/v1/metrics/timeseries/builds", get(api_metrics_builds))
    .route(
      "/api/v1/metrics/timeseries/duration",
      get(api_metrics_duration),
    )
    .route("/api/v1/metrics/systems", get(api_metrics_systems))
    .route("/", get(home))
    .route("/projects", get(projects))
    .route("/projects/new", get(project_setup))
    .route("/project/{id}", get(project))
    .route("/jobset/{id}", get(jobset))
    .route("/jobset/{id}/jobs", get(jobset_jobs))
    .route("/evaluations", get(evaluations))
    .route("/evaluation/{id}", get(evaluation))
    .route("/builds", get(builds))
    .route("/build/{id}", get(build))
    .route("/queue", get(queue))
    .route("/channels", get(channels))
    .route("/channel/{id}", get(channel))
    .route("/news", get(news))
    .route("/admin", get(admin))
    .route("/users", get(users))
    .route("/starred", get(starred))
    .route("/metrics", get(metrics))
    .route("/login", get(login))
    .route("/private", get(private))
}

fn render<T: Template>(template: T) -> Response {
  match template.render() {
    Ok(html) => Html(html).into_response(),
    Err(error) => {
      (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("Template error: {error}"),
      )
        .into_response()
    },
  }
}

async fn index() -> Redirect {
  Redirect::temporary("/")
}

async fn theme_css() -> Response {
  Response::builder()
    .header(header::CONTENT_TYPE, "text/css")
    .header(header::CACHE_CONTROL, "no-cache")
    .body(Body::from(
      ":root {\n  --accent: #111827;\n  --accent-hover: #000000;\n  \
       --accent-strong: #374151;\n}\n",
    ))
    .unwrap_or_else(|error| {
      Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::from(format!("response builder failed: {error}")))
        .unwrap_or_else(|_| Response::new(Body::empty()))
    })
}

fn static_dir() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static")
}

async fn api_projects() -> Json<serde_json::Value> {
  Json(serde_json::json!({
    "data": [
      { "id": "00000000-0000-0000-0000-000000000001", "name": "circus" }
    ]
  }))
}

async fn api_metrics_builds() -> Json<serde_json::Value> {
  Json(serde_json::json!({
    "timestamps": [
      "2026-06-18T08:00:00Z",
      "2026-06-18T09:00:00Z",
      "2026-06-18T10:00:00Z",
      "2026-06-18T11:00:00Z",
      "2026-06-18T12:00:00Z"
    ],
    "total": [12, 18, 14, 22, 16],
    "failed": [1, 2, 0, 3, 1]
  }))
}

async fn api_metrics_duration() -> Json<serde_json::Value> {
  Json(serde_json::json!({
    "timestamps": [
      "2026-06-18T08:00:00Z",
      "2026-06-18T09:00:00Z",
      "2026-06-18T10:00:00Z",
      "2026-06-18T11:00:00Z",
      "2026-06-18T12:00:00Z"
    ],
    "p50": [45, 52, 48, 61, 55],
    "p95": [180, 210, 195, 240, 220],
    "p99": [300, 340, 310, 380, 350]
  }))
}

async fn api_metrics_systems() -> Json<serde_json::Value> {
  Json(serde_json::json!({
    "systems": ["x86_64-linux", "aarch64-linux"],
    "counts": [42, 18]
  }))
}

async fn home() -> Response {
  render(HomeTemplate {
    ui:                 ui(),
    total_builds:       1842,
    completed_builds:   1710,
    failed_builds:      27,
    running_builds:     3,
    pending_builds:     19,
    recent_builds:      builds_fixture(),
    failed_builds_list: vec![build_view(
      5,
      "packages.aarch64-linux.server",
      "Failed",
      "failed",
    )],
    recent_evals:       evals_fixture(),
    projects:           project_summaries(),
    queue_by_system:    vec![
      QueueSystemView {
        system: "x86_64-linux".into(),
        count:  12,
      },
      QueueSystemView {
        system: "aarch64-linux".into(),
        count:  7,
      },
    ],
    workers:            vec![
      WorkerSummaryView {
        name:         "agent-fast-01".into(),
        system:       "x86_64-linux".into(),
        status_text:  "busy".into(),
        status_class: "running".into(),
        current_jobs: 2,
        max_jobs:     4,
      },
      WorkerSummaryView {
        name:         "agent-arm-01".into(),
        system:       "aarch64-linux".into(),
        status_text:  "idle".into(),
        status_class: "completed".into(),
        current_jobs: 0,
        max_jobs:     2,
      },
    ],
    worker_online:      2,
    worker_total:       3,
    refreshed_at:       "2026-06-18 12:00 UTC".into(),
    announcements:      news_items(),
    is_admin:           true,
    auth_name:          "operator".into(),
  })
}

async fn projects() -> Response {
  render(ProjectsTemplate {
    ui:          ui(),
    projects:    vec![project_fixture()],
    limit:       20,
    has_prev:    false,
    has_next:    false,
    prev_offset: 0,
    next_offset: 20,
    page:        1,
    total_pages: 1,
    is_admin:    true,
    auth_name:   "operator".into(),
    csrf_token:  csrf(),
  })
}

async fn project_setup() -> Response {
  render(ProjectSetupTemplate {
    ui:         ui(),
    is_admin:   true,
    auth_name:  "operator".into(),
    csrf_token: csrf(),
  })
}

async fn project() -> Response {
  render(ProjectTemplate {
    ui:           ui(),
    project:      project_fixture(),
    jobsets:      vec![jobset_fixture()],
    recent_evals: evals_fixture(),
    is_admin:     true,
    auth_name:    "operator".into(),
    csrf_token:   csrf(),
  })
}

async fn jobset() -> Response {
  render(JobsetTemplate {
    ui:             ui(),
    project:        project_fixture(),
    jobset:         jobset_fixture(),
    eval_summaries: eval_summaries(),
    is_admin:       true,
    auth_name:      "operator".into(),
    csrf_token:     csrf(),
  })
}

async fn jobset_jobs() -> Response {
  render(JobsetJobsTemplate {
    ui:            ui(),
    project:       project_fixture(),
    jobset:        jobset_fixture(),
    columns:       job_columns(),
    rows:          job_rows(),
    show_inactive: false,
    is_admin:      true,
    auth_name:     "operator".into(),
  })
}

async fn evaluations() -> Response {
  render(EvaluationsTemplate {
    ui:          ui(),
    evals:       evals_fixture(),
    limit:       20,
    has_prev:    false,
    has_next:    false,
    prev_offset: 0,
    next_offset: 20,
    page:        1,
    total_pages: 1,
    is_admin:    true,
    auth_name:   "operator".into(),
    csrf_token:  csrf(),
  })
}

async fn evaluation() -> Response {
  render(EvaluationTemplate {
    ui:              ui(),
    eval:            eval_view(3, "Completed", "completed"),
    builds:          builds_fixture(),
    project_name:    "circus".into(),
    project_id:      id(1),
    jobset_name:     "packages".into(),
    jobset_id:       id(2),
    succeeded_count: 2,
    failed_count:    1,
    running_count:   1,
    pending_count:   1,
    is_admin:        true,
    auth_name:       "operator".into(),
    csrf_token:      csrf(),
  })
}

async fn builds() -> Response {
  render(BuildsTemplate {
    ui:            ui(),
    builds:        builds_fixture(),
    limit:         20,
    has_prev:      false,
    has_next:      false,
    prev_offset:   0,
    next_offset:   20,
    page:          1,
    total_pages:   1,
    filter_status: String::new(),
    filter_system: String::new(),
    filter_job:    String::new(),
    is_admin:      true,
    auth_name:     "operator".into(),
  })
}

async fn build() -> Response {
  let build_id = id(4);
  render(BuildTemplate {
    ui:                ui(),
    build:             build_view(
      4,
      "packages.x86_64-linux.circus-server",
      "Succeeded",
      "completed",
    ),
    builder_label:     "agent-fast-01".into(),
    steps:             vec![BuildStep {
      id: id(31),
      build_id,
      step_number: 1,
      command: "nix build .#circus-server".into(),
      output: Some("building '/nix/store/...-circus-server.drv'".into()),
      error_output: None,
      started_at: Utc::now() - Duration::minutes(3),
      completed_at: Some(Utc::now() - Duration::minutes(1)),
      exit_code: Some(0),
    }],
    products:          vec![BuildProduct {
      id: id(32),
      build_id,
      name: "out".into(),
      path: "/nix/store/preview-circus-server".into(),
      sha256_hash: Some("sha256-preview".into()),
      file_size: Some(42_000_000),
      content_type: Some("application/x-nix-archive".into()),
      is_directory: true,
      gc_root_path: Some("/nix/var/nix/gcroots/circus/preview".into()),
      created_at: Utc::now() - Duration::minutes(1),
    }],
    dependencies:      vec![build_view(
      6,
      "checks.x86_64-linux.config",
      "Succeeded",
      "completed",
    )],
    dependents:        Vec::new(),
    eval_id:           id(3),
    eval_commit_short: "9f2c7a113bad".into(),
    jobset_id:         id(2),
    jobset_name:       "packages".into(),
    project_id:        id(1),
    project_name:      "circus".into(),
    is_admin:          true,
    auth_name:         "operator".into(),
  })
}

async fn queue() -> Response {
  render(QueueTemplate {
    ui:             ui(),
    pending_builds: vec![queue_build(
      7,
      "packages.aarch64-linux.agent",
      None,
      1,
    )],
    running_builds: vec![queue_build(
      8,
      "checks.x86_64-linux.integration",
      Some("agent-fast-01"),
      0,
    )],
    pending_count:  1,
    running_count:  1,
    permissions:    permissions(),
    csrf_token:     csrf(),
    is_admin:       true,
    auth_name:      "operator".into(),
  })
}

async fn channels() -> Response {
  render(ChannelsTemplate {
    ui:        ui(),
    channels:  vec![ChannelView {
      id:                    id(5),
      name:                  "latest".into(),
      current_evaluation_id: Some(id(3)),
      updated_at:            "2026-06-18 12:02 UTC".into(),
      status_text:           "Completed".into(),
      status_class:          "completed".into(),
      job_count:             3,
    }],
    is_admin:  true,
    auth_name: "operator".into(),
  })
}

async fn channel() -> Response {
  render(ChannelTemplate {
    ui:              ui(),
    channel:         channel_fixture(),
    builds:          builds_fixture(),
    succeeded_count: 2,
    failed_count:    1,
    pending_count:   1,
    is_admin:        true,
    auth_name:       "operator".into(),
  })
}

async fn news() -> Response {
  render(NewsTemplate {
    ui:         ui(),
    items:      news_items(),
    is_admin:   true,
    auth_name:  "operator".into(),
    csrf_token: csrf(),
  })
}

async fn admin() -> Response {
  render(AdminTemplate {
    ui:                      ui(),
    status:                  SystemStatus {
      projects_count:    4,
      jobsets_count:     9,
      evaluations_count: 241,
      builds_pending:    19,
      builds_running:    3,
      builds_completed:  1710,
      builds_failed:     27,
      remote_builders:   3,
      channels_count:    2,
    },
    builders:                vec![BuilderView {
      id:             id(41),
      name:           "legacy-builder".into(),
      ssh_uri:        "ssh://builder@host".into(),
      systems:        "x86_64-linux".into(),
      max_jobs:       4,
      enabled:        true,
      current_builds: 1,
      load_percent:   25,
      last_activity:  "2m ago".into(),
    }],
    agents:                  vec![AgentView {
      machine_id:       id(42),
      name:             "agent-fast-01".into(),
      hostname:         "agent-fast-01".into(),
      systems:          "x86_64-linux".into(),
      max_jobs:         4,
      current_jobs:     2,
      connected:        true,
      builds_succeeded: 128,
      builds_failed:    3,
      last_seen:        "just now".into(),
      last_seen_sort:   0,
    }],
    agent_sort_headers:      vec![SortHeaderView {
      key:         "name".into(),
      label:       "Name".into(),
      href:        "/admin?agent_sort=name".into(),
      default_dir: "asc".into(),
      active:      true,
      indicator:   "↑".into(),
      aria_sort:   "ascending".into(),
    }],
    agent_sort_key:          "name".into(),
    agent_sort_dir:          "asc".into(),
    api_keys:                vec![ApiKeyView {
      id:           id(43),
      name:         "preview-admin".into(),
      role:         "admin".into(),
      created_at:   "2026-06-18".into(),
      last_used_at: "never".into(),
    }],
    notification_tasks:      vec![NotificationTaskView {
      id:                id(44),
      notification_type: "webhook".into(),
      status:            "pending".into(),
      attempts:          1,
      max_attempts:      5,
      next_retry_at:     "in 3m".into(),
      last_error:        String::new(),
      created_at:        "2026-06-18 12:00".into(),
    }],
    pinned_outputs:          vec![PinnedOutputView {
      build_id:           id(4),
      product_id:         id(32),
      job_name:           "packages.x86_64-linux.circus-server".into(),
      system:             "x86_64-linux".into(),
      status:             "succeeded".into(),
      product_name:       "out".into(),
      path:               "/nix/store/preview-circus-server".into(),
      gc_root_path:       "/nix/var/nix/gcroots/circus/preview".into(),
      product_created_at: "2026-06-18 12:00".into(),
    }],
    config_path:             "preview://circus.toml".into(),
    config_contents:         "[server]\nport = 3000\n".into(),
    config_editable:         false,
    config_read_only_reason: "Preview mode does not edit configuration".into(),
    is_admin:                true,
    auth_name:               "operator".into(),
    csrf_token:              csrf(),
  })
}

async fn users() -> Response {
  render(UsersTemplate {
    ui:          ui(),
    users:       vec![UserView {
      id:            id(51),
      username:      "operator".into(),
      email:         "operator@example.invalid".into(),
      role:          "admin".into(),
      user_type:     "local".into(),
      enabled:       true,
      last_login_at: "2026-06-18 12:00".into(),
    }],
    limit:       20,
    has_prev:    false,
    has_next:    false,
    prev_offset: 0,
    next_offset: 20,
    page:        1,
    total_pages: 1,
    is_admin:    true,
    auth_name:   "operator".into(),
    csrf_token:  csrf(),
  })
}

async fn starred() -> Response {
  render(StarredTemplate {
    ui:           ui(),
    starred_jobs: vec![StarredJobView {
      id:              id(61),
      project_id:      id(1),
      project_name:    "circus".into(),
      jobset_id:       Some(id(2)),
      jobset_name:     "packages".into(),
      job_name:        "packages.x86_64-linux.circus-server".into(),
      status_text:     "Succeeded".into(),
      status_class:    "completed".into(),
      latest_build_id: Some(id(4)),
    }],
    is_logged_in: true,
    is_admin:     true,
    auth_name:    "operator".into(),
    csrf_token:   csrf(),
  })
}

async fn metrics() -> Response {
  render(MetricsTemplate {
    ui:        ui(),
    is_admin:  true,
    auth_name: "operator".into(),
  })
}

async fn login() -> Response {
  render(LoginTemplate {
    ui:        ui(),
    error:     Some("Preview mode accepts no credentials.".into()),
    is_admin:  false,
    auth_name: String::new(),
  })
}

async fn private() -> Response {
  render(PrivateTemplate {
    ui:        ui(),
    is_admin:  false,
    auth_name: String::new(),
  })
}

fn id(n: u128) -> Uuid {
  Uuid::from_u128(n)
}

fn ui() -> UiTemplateConfig {
  let config = UiConfig {
    brand_name: "Circus Preview".into(),
    brand_subtitle: "Fixture-backed frontend".into(),
    ..UiConfig::default()
  };
  UiTemplateConfig::from_config(&config)
}

fn csrf() -> String {
  "preview-csrf-token".into()
}

fn permissions() -> UiPermissions {
  UiPermissions {
    admin:           true,
    bump_to_front:   true,
    cancel_build:    true,
    restart_jobs:    true,
    create_projects: true,
    eval_jobset:     true,
  }
}

fn project_fixture() -> Project {
  Project {
    id:              id(1),
    name:            "circus".into(),
    description:     Some("Nix-native CI control plane".into()),
    repository_url:  "https://github.com/manic-systems/circus".into(),
    cache_enabled:   true,
    cache_url:       Some("https://cache.example.invalid".into()),
    cache_upstreams: SqlxJson(BinaryCacheUpstreams::default()),
    created_at:      Utc::now() - Duration::days(30),
    updated_at:      Utc::now() - Duration::minutes(5),
  }
}

fn jobset_fixture() -> Jobset {
  Jobset {
    id:                id(2),
    project_id:        id(1),
    name:              "packages".into(),
    nix_expression:    "packages".into(),
    enabled:           true,
    flake_mode:        true,
    check_interval:    600,
    trigger_mode:      JobsetTriggerMode::SourceChange,
    branch:            Some("main".into()),
    branch_pattern:    None,
    tag_pattern:       None,
    scheduling_shares: 100,
    created_at:        Utc::now() - Duration::days(20),
    updated_at:        Utc::now() - Duration::minutes(5),
    state:             JobsetState::Enabled,
    last_checked_at:   Some(Utc::now() - Duration::minutes(10)),
    keep_nr:           3,
  }
}

fn channel_fixture() -> Channel {
  Channel {
    id:                    id(5),
    project_id:            id(1),
    name:                  "latest".into(),
    jobset_id:             id(2),
    current_evaluation_id: Some(id(3)),
    created_at:            Utc::now() - Duration::days(7),
    updated_at:            Utc::now() - Duration::minutes(2),
  }
}

fn news_items() -> Vec<NewsItem> {
  vec![NewsItem {
    id:         id(21),
    title:      "Preview fixtures updated".into(),
    content:    "Frontend previews are served from xtask without a VM.".into(),
    created_by: Some(id(51)),
    created_at: Utc::now() - Duration::hours(2),
  }]
}

fn build_view(n: u128, job: &str, status: &str, class: &str) -> BuildView {
  BuildView {
    id:            id(n),
    job_name:      job.into(),
    project_id:    Some(id(1)),
    project_name:  "circus".into(),
    jobset_id:     Some(id(2)),
    jobset_name:   "packages".into(),
    status_text:   status.into(),
    status_class:  class.into(),
    system:        "x86_64-linux".into(),
    created_at:    "2026-06-18 11:45".into(),
    started_at:    "2026-06-18 11:46".into(),
    completed_at:  if class == "running" {
      String::new()
    } else {
      "2026-06-18 11:49".into()
    },
    duration:      "3m 12s".into(),
    started_epoch: if class == "running" {
      Some(Utc::now().timestamp() - 90)
    } else {
      None
    },
    priority:      100,
    is_aggregate:  false,
    signed:        true,
    drv_path:      "/nix/store/preview-circus-server.drv".into(),
    output_path:   "/nix/store/preview-circus-server".into(),
    error_message: if class == "failed" {
      "error: builder failed with exit code 1".into()
    } else {
      String::new()
    },
    error_lines:   if class == "failed" {
      vec![BuildErrorLine {
        text:  "builder failed with exit code 1".into(),
        level: "error",
      }]
    } else {
      Vec::new()
    },
    has_log:       true,
  }
}

fn builds_fixture() -> Vec<BuildView> {
  vec![
    build_view(
      4,
      "packages.x86_64-linux.circus-server",
      "Succeeded",
      "completed",
    ),
    build_view(5, "packages.aarch64-linux.server", "Failed", "failed"),
    build_view(6, "checks.x86_64-linux.integration", "Running", "running"),
  ]
}

fn queue_build(
  n: u128,
  job: &str,
  builder: Option<&str>,
  pos: i64,
) -> QueueBuildView {
  QueueBuildView {
    id:            id(n),
    job_name:      job.into(),
    project_id:    Some(id(1)),
    project_name:  "circus".into(),
    jobset_id:     Some(id(2)),
    jobset_name:   "packages".into(),
    system:        "x86_64-linux".into(),
    created_at:    "2026-06-18 11:55".into(),
    started_at:    if builder.is_some() {
      "2026-06-18 11:56".into()
    } else {
      String::new()
    },
    elapsed:       "1m 30s".into(),
    started_epoch: builder.map(|_| Utc::now().timestamp() - 90),
    priority:      100,
    builder_name:  builder.map(str::to_string),
    queue_pos:     pos,
  }
}

fn eval_view(n: u128, status: &str, class: &str) -> EvalView {
  EvalView {
    id:            id(n),
    commit_hash:   "9f2c7a113badf00d7e57c0ffee1234567890abcd".into(),
    commit_short:  "9f2c7a113bad".into(),
    status_text:   status.into(),
    status_class:  class.into(),
    time:          "2026-06-18 11:42".into(),
    error_message: None,
    hidden:        false,
    jobset_name:   "packages".into(),
    project_name:  "circus".into(),
  }
}

fn evals_fixture() -> Vec<EvalView> {
  vec![
    eval_view(3, "Completed", "completed"),
    eval_view(13, "Running", "running"),
  ]
}

fn eval_summaries() -> Vec<EvalSummaryView> {
  vec![EvalSummaryView {
    id:           id(3),
    commit_short: "9f2c7a113bad".into(),
    status_text:  "Completed".into(),
    status_class: "completed".into(),
    time:         "2026-06-18 11:42".into(),
    succeeded:    18,
    failed:       1,
    pending:      0,
    hidden:       false,
  }]
}

fn project_summaries() -> Vec<ProjectSummaryView> {
  vec![ProjectSummaryView {
    id:               id(1),
    name:             "circus".into(),
    jobset_count:     2,
    last_eval_status: "Completed".into(),
    last_eval_class:  "completed".into(),
    last_eval_time:   "2026-06-18 11:42".into(),
    failing_jobs:     1,
    queued_jobs:      3,
    systems:          "x86_64-linux, aarch64-linux".into(),
    updated_at:       "2026-06-18 11:50".into(),
  }]
}

fn job_columns() -> Vec<JobStatusColumn> {
  vec![
    JobStatusColumn {
      eval_id: id(3),
      label:   "9f2c7a".into(),
      title:   "9f2c7a113bad".into(),
    },
    JobStatusColumn {
      eval_id: id(13),
      label:   "running".into(),
      title:   "running evaluation".into(),
    },
  ]
}

fn job_rows() -> Vec<JobStatusRow> {
  vec![JobStatusRow {
    job_name:  "packages.x86_64-linux.circus-server".into(),
    is_active: true,
    cells:     vec![
      JobStatusCell {
        href:         "/build/00000000-0000-0000-0000-000000000004".into(),
        status_text:  "Succeeded".into(),
        status_class: "completed".into(),
      },
      JobStatusCell {
        href:         "/build/00000000-0000-0000-0000-000000000006".into(),
        status_text:  "Running".into(),
        status_class: "running".into(),
      },
    ],
  }]
}
