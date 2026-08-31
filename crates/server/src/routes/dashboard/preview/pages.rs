use axum::{extract::Query, response::Response};
use chrono::{Duration, Utc};
use circus_common::models::{BuildProduct, SystemStatus};

use super::{
  super::{
    shared::{
      ApiKeyView,
      PrivateTemplate,
      QueueSystemView,
      StarredJobView,
      UserView,
      WorkerSummaryView,
    },
    templates::{
      AdminTemplate,
      AgentView,
      BuildTemplate,
      BuildsTemplate,
      CacheDetailTemplate,
      CacheNarsTemplate,
      CacheRowView,
      CachesTemplate,
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
      NarRowView,
      NewsTemplate,
      NotificationTaskView,
      NotificationsTemplate,
      PinnedOutputView,
      ProjectSetupTemplate,
      ProjectTemplate,
      ProjectsTemplate,
      QueueTemplate,
      SortHeaderView,
      StarredTemplate,
      UsersTemplate,
    },
  },
  fixtures::{
    self,
    builds_fixture,
    channel_fixture,
    csrf,
    eval_summaries,
    evals_fixture,
    id,
    job_columns,
    job_rows,
    jobset_fixture,
    news_items,
    permissions,
    project_fixture,
    project_summaries,
    queue_build,
    ui,
  },
  render,
};

#[derive(serde::Deserialize)]
pub(super) struct PreviewBuildFilterParams {
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
}

pub(super) async fn home() -> Response {
  render(HomeTemplate {
    ui:                 ui(),
    total_builds:       1842,
    completed_builds:   1710,
    failed_builds:      27,
    running_builds:     3,
    pending_builds:     19,
    recent_builds:      builds_fixture(),
    failed_builds_list: vec![fixtures::build_view(
      5,
      "packages.aarch64-linux.circus-server",
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
    system_filters:     vec!["aarch64-linux".into(), "x86_64-linux".into()],
    worker_online:      2,
    worker_total:       3,
    refreshed_at:       "2026-06-18 12:00 UTC".into(),
    announcements:      news_items(),
    is_admin:           true,
    auth_name:          "operator".into(),
  })
}

pub(super) async fn projects() -> Response {
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

pub(super) async fn project_setup() -> Response {
  render(ProjectSetupTemplate {
    ui:         ui(),
    is_admin:   true,
    auth_name:  "operator".into(),
    csrf_token: csrf(),
  })
}

pub(super) async fn notifications() -> Response {
  render(NotificationsTemplate {
    ui:              ui(),
    project:         project_fixture(),
    configs:         Vec::new(),
    project_mutable: true,
    is_admin:        true,
    auth_name:       "operator".into(),
    csrf_token:      csrf(),
  })
}

pub(super) async fn project() -> Response {
  render(ProjectTemplate {
    ui:              ui(),
    project:         project_fixture(),
    jobsets:         vec![jobset_fixture()],
    recent_evals:    evals_fixture(),
    project_mutable: true,
    is_admin:        true,
    auth_name:       "operator".into(),
    csrf_token:      csrf(),
  })
}

pub(super) async fn jobset() -> Response {
  render(JobsetTemplate {
    ui:              ui(),
    project:         project_fixture(),
    jobset:          jobset_fixture(),
    eval_summaries:  eval_summaries(),
    project_mutable: true,
    is_admin:        true,
    auth_name:       "operator".into(),
    csrf_token:      csrf(),
  })
}

pub(super) async fn jobset_jobs() -> Response {
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

pub(super) async fn evaluations() -> Response {
  render(EvaluationsTemplate {
    ui:             ui(),
    evals:          evals_fixture(),
    filter_project: String::new(),
    filter_jobset:  String::new(),
    filter_commit:  String::new(),
    filter_status:  String::new(),
    limit:          20,
    has_prev:       false,
    has_next:       false,
    prev_offset:    0,
    next_offset:    20,
    page:           1,
    total_pages:    1,
    is_admin:       true,
    auth_name:      "operator".into(),
    csrf_token:     csrf(),
  })
}

pub(super) async fn evaluation() -> Response {
  let failed_derivations = vec![
    fixtures::build_view(
      5,
      "packages.aarch64-linux.circus-server",
      "Failed",
      "failed",
    ),
    fixtures::build_view(7, "drv:0vdd2i8j-intermediate", "Failed", "failed"),
  ];
  render(EvaluationTemplate {
    ui: ui(),
    eval: fixtures::eval_view(3, "Completed", "completed"),
    builds: builds_fixture(),
    failed_derivations,
    project_name: "circus".into(),
    project_id: id(1),
    jobset_name: "packages".into(),
    jobset_id: id(2),
    succeeded_count: 2,
    failed_count: 1,
    running_count: 1,
    pending_count: 1,
    is_admin: true,
    auth_name: "operator".into(),
    csrf_token: csrf(),
  })
}

pub(super) async fn builds(
  Query(params): Query<PreviewBuildFilterParams>,
) -> Response {
  let status = params.status.unwrap_or_default();
  let system = params.system.unwrap_or_default();
  let job_name = params.job_name.unwrap_or_default();
  let status_filter = status.to_lowercase();
  let system_filter = system.to_lowercase();
  let job_filter = job_name.to_lowercase();
  let builds = builds_fixture()
    .into_iter()
    .filter(|build| {
      let status_matches = status_filter.is_empty()
        || build.status_class == status_filter
        || (status_filter == "succeeded" && build.status_class == "completed");
      let system_matches = system_filter.is_empty()
        || build.system.to_lowercase().contains(&system_filter);
      let job_matches = job_filter.is_empty()
        || build.job_name.to_lowercase().contains(&job_filter);

      status_matches && system_matches && job_matches
    })
    .collect();

  render(BuildsTemplate {
    ui: ui(),
    builds,
    limit: 20,
    has_prev: false,
    has_next: false,
    prev_offset: 0,
    next_offset: 20,
    page: 1,
    total_pages: 1,
    filter_status: status,
    filter_system: system,
    filter_job: job_name,
    is_admin: true,
    auth_name: "operator".into(),
  })
}

pub(super) async fn build() -> Response {
  let build_id = id(4);
  render(BuildTemplate {
    ui:                ui(),
    build:             fixtures::build_view(
      4,
      "packages.x86_64-linux.circus-server",
      "Succeeded",
      "completed",
    ),
    builder_label:     "agent-fast-01".into(),
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
    dependencies:      vec![fixtures::build_view(
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

pub(super) async fn queue() -> Response {
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
    show_running:   true,
    show_pending:   true,
    filter_status:  String::new(),
    filter_system:  String::new(),
    filter_job:     String::new(),
    permissions:    permissions(),
    csrf_token:     csrf(),
    is_admin:       true,
    auth_name:      "operator".into(),
  })
}

pub(super) async fn channels() -> Response {
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

pub(super) async fn channel() -> Response {
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

pub(super) async fn news() -> Response {
  render(NewsTemplate {
    ui:         ui(),
    items:      news_items(),
    is_admin:   true,
    auth_name:  "operator".into(),
    csrf_token: csrf(),
  })
}

pub(super) async fn admin() -> Response {
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
      channels_count:    2,
    },
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
    gc_enabled:              true,
    gc_requested:            false,
    is_admin:                true,
    auth_name:               "operator".into(),
    csrf_token:              csrf(),
  })
}

pub(super) async fn users() -> Response {
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

pub(super) async fn starred() -> Response {
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

pub(super) async fn metrics() -> Response {
  render(MetricsTemplate {
    ui:        ui(),
    is_admin:  true,
    auth_name: "operator".into(),
  })
}

pub(super) async fn login() -> Response {
  render(LoginTemplate {
    ui:        ui(),
    error:     Some("Preview mode accepts no credentials.".into()),
    is_admin:  false,
    auth_name: String::new(),
  })
}

pub(super) async fn private() -> Response {
  render(PrivateTemplate {
    ui:        ui(),
    is_admin:  false,
    auth_name: String::new(),
  })
}

pub(super) async fn caches() -> Response {
  render(CachesTemplate {
    ui:                 ui(),
    is_admin:           true,
    auth_name:          "operator".into(),
    total_nars:         42,
    total_compressed:   "12.3 MiB".into(),
    total_uncompressed: "45.6 MiB".into(),
    caches:             vec![
      CacheRowView {
        name:              "global".into(),
        scope_label:       "Global".into(),
        active:            true,
        nar_count:         30,
        compressed:        "8.1 MiB".into(),
        requests_per_hour: 142,
        detail_href:       "/caches/global".into(),
      },
      CacheRowView {
        name:              "circus".into(),
        scope_label:       "Project".into(),
        active:            true,
        nar_count:         12,
        compressed:        "4.2 MiB".into(),
        requests_per_hour: 37,
        detail_href:       "/caches/circus".into(),
      },
    ],
  })
}

pub(super) async fn cache_detail() -> Response {
  render(CacheDetailTemplate {
    ui:                     ui(),
    is_admin:               true,
    auth_name:              "operator".into(),
    name:                   "global".into(),
    scope_label:            "Global".into(),
    active:                 true,
    nars_href:              "/caches/global/nars".into(),
    storage_timeseries_url: "/api/v1/admin/caches/global/storage-timeseries"
      .into(),
    traffic_timeseries_url: "/api/v1/admin/caches/global/traffic-timeseries"
      .into(),
    packages_stored:        30,
    uncompressed:           "45.6 MiB".into(),
    compressed:             "8.1 MiB".into(),
    requests_last_hour:     142,
    traffic_last_hour:      "3.2 MiB".into(),
    has_substituter:        true,
    substituter_url:        "https://cache.example.invalid".into(),
    has_public_key:         true,
    public_key:             "cache.example.invalid-1:\
                             AbCdEfGhIjKlMnOpQrStUvWxYz1234567890+ab="
      .into(),
    has_snippet:            true,
    nix_conf_snippet:
      "substituters = https://cache.example.invalid\ntrusted-public-keys = \
       cache.example.invalid-1:AbCdEfGhIjKlMnOpQrStUvWxYz1234567890+ab="
        .into(),
    csrf_token:             csrf(),
    gc_notice:              String::new(),
    gc_error:               false,
    is_global:              true,
  })
}

pub(super) async fn cache_nars() -> Response {
  render(CacheNarsTemplate {
    ui:             ui(),
    is_admin:       true,
    auth_name:      "operator".into(),
    name:           "global".into(),
    scope_label:    "Global".into(),
    detail_href:    "/caches/global".into(),
    filter_hash:    String::new(),
    filter_package: String::new(),
    total_nars:     30,
    nar_size:       "45.6 MiB".into(),
    file_size:      "8.1 MiB".into(),
    last_uploaded:  "2026-06-18 12:00 UTC".into(),
    oldest_fetched: "2026-06-18 11:30 UTC".into(),
    nars:           vec![
      NarRowView {
        hash:         "9f2c7a113badf00d7e57c".into(),
        package:      "circus-server".into(),
        store_path:   "/nix/store/9f2c7a113badf00d7e57c-circus-server".into(),
        nar_size:     "1.5 MiB".into(),
        compressed:   "420 KiB".into(),
        created_at:   "2026-06-18 11:45".into(),
        last_fetched: "2026-06-18 11:50".into(),
      },
      NarRowView {
        hash:         "a1b2c3d4e5f6a7b8c9d0".into(),
        package:      "circus-agent".into(),
        store_path:   "/nix/store/a1b2c3d4e5f6a7b8c9d0-circus-agent".into(),
        nar_size:     "2.1 MiB".into(),
        compressed:   "680 KiB".into(),
        created_at:   "2026-06-18 11:30".into(),
        last_fetched: "2026-06-18 11:45".into(),
      },
    ],
    page:           1,
    total_pages:    2,
    has_prev:       false,
    has_next:       true,
    prev_offset:    0,
    next_offset:    20,
    limit:          20,
  })
}
