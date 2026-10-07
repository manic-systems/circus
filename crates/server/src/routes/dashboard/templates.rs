//! Askama template structs for every dashboard page. The structs are
//! field-private from outside this module, but `pub(super)` for sibling
//! handler modules. Each `#[derive(Template)]` macro looks for the
//! `path = "..."` HTML template under the configured templates root.
#![expect(
  dead_code,
  reason = "Askama templates read fields from generated render impls"
)]

use askama::Template;
use circus_common::models::Project;

pub(super) use super::shared::UiTemplateConfig;

#[derive(Template)]
#[template(path = "projects.html")]
pub(super) struct ProjectsTemplate {
  pub(super) ui:          UiTemplateConfig,
  pub(super) projects:    Vec<Project>,
  pub(super) limit:       i64,
  pub(super) has_prev:    bool,
  pub(super) has_next:    bool,
  pub(super) prev_offset: i64,
  pub(super) next_offset: i64,
  pub(super) page:        i64,
  pub(super) total_pages: i64,
  pub(super) is_admin:    bool,
  pub(super) auth_name:   String,
  pub(super) csrf_token:  String,
}

pub(super) struct SortHeaderView {
  pub(super) key:         String,
  pub(super) label:       String,
  pub(super) href:        String,
  pub(super) default_dir: String,
  pub(super) active:      bool,
  pub(super) indicator:   String,
  pub(super) aria_sort:   String,
}

#[derive(Template)]
#[template(path = "project_setup.html")]
pub(super) struct ProjectSetupTemplate {
  pub(super) ui:         UiTemplateConfig,
  pub(super) is_admin:   bool,
  pub(super) auth_name:  String,
  pub(super) csrf_token: String,
}

#[derive(Template)]
#[template(path = "metrics.html")]
pub(super) struct MetricsTemplate {
  pub(super) ui:        UiTemplateConfig,
  pub(super) is_admin:  bool,
  pub(super) auth_name: String,
}

#[derive(Template)]
#[template(path = "cache_detail.html")]
#[expect(
  clippy::struct_excessive_bools,
  reason = "template render flags for optional how-to-use fields; not state"
)]
pub(super) struct CacheDetailTemplate {
  pub(super) ui:                     UiTemplateConfig,
  pub(super) is_admin:               bool,
  pub(super) auth_name:              String,
  pub(super) name:                   String,
  pub(super) scope_label:            String,
  pub(super) active:                 bool,
  pub(super) nars_href:              String,
  pub(super) storage_timeseries_url: String,
  pub(super) traffic_timeseries_url: String,
  pub(super) packages_stored:        i64,
  pub(super) uncompressed:           String,
  pub(super) compressed:             String,
  pub(super) requests_last_hour:     i64,
  pub(super) traffic_last_hour:      String,
  pub(super) has_substituter:        bool,
  pub(super) substituter_url:        String,
  pub(super) has_public_key:         bool,
  pub(super) public_key:             String,
  pub(super) has_snippet:            bool,
  pub(super) nix_conf_snippet:       String,
  pub(super) csrf_token:             String,
  pub(super) gc_notice:              String,
  pub(super) gc_error:               bool,
  pub(super) is_global:              bool,
}
