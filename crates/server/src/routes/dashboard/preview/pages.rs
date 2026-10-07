use axum::response::Response;

use super::{
  super::{
    shared::PrivateTemplate,
    templates::{
      CacheDetailTemplate,
      MetricsTemplate,
      ProjectSetupTemplate,
      ProjectsTemplate,
    },
  },
  fixtures::{csrf, project_fixture, ui},
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
