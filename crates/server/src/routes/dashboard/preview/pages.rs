use axum::response::Response;
use circus_common::models::SystemStatus;

use super::{
  super::{
    shared::{ApiKeyView, LinkedIdentityView, PrivateTemplate, UserView},
    templates::{
      AdminTemplate,
      AgentView,
      CacheDetailTemplate,
      MetricsTemplate,
      NotificationTaskView,
      NotificationsTemplate,
      PinnedOutputView,
      ProjectSetupTemplate,
      ProjectTemplate,
      ProjectsTemplate,
      SortHeaderView,
      UsersTemplate,
    },
  },
  fixtures::{csrf, evals_fixture, id, jobset_fixture, project_fixture, ui},
  render,
};

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
    repository_page: url::Url::parse("https://github.com/manic-systems/circus")
      .ok(),
    jobsets:         vec![jobset_fixture()],
    recent_evals:    evals_fixture(),
    project_mutable: true,
    is_admin:        true,
    auth_name:       "operator".into(),
    csrf_token:      csrf(),
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
      linked:        vec![LinkedIdentityView {
        provider: "pocketid".into(),
        label:    "PocketID".into(),
      }],
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

pub(super) async fn metrics() -> Response {
  render(MetricsTemplate {
    ui:        ui(),
    is_admin:  true,
    auth_name: "operator".into(),
  })
}

pub(super) async fn private() -> Response {
  render(PrivateTemplate {
    ui:        ui(),
    is_admin:  false,
    auth_name: String::new(),
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
