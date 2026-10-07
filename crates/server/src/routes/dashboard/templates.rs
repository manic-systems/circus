//! Askama template structs for every dashboard page. The structs are
//! field-private from outside this module, but `pub(super)` for sibling
//! handler modules. Each `#[derive(Template)]` macro looks for the
//! `path = "..."` HTML template under the configured templates root.
#![expect(
  dead_code,
  reason = "Askama templates read fields from generated render impls"
)]

use askama::Template;
use circus_common::models::{Jobset, Project, SystemStatus};
use uuid::Uuid;

pub(super) use super::shared::UiTemplateConfig;
use super::shared::{ApiKeyView, EvalView, UserView};

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

#[derive(Template)]
#[template(path = "project.html")]
pub(super) struct ProjectTemplate {
  pub(super) ui:              UiTemplateConfig,
  pub(super) project:         Project,
  pub(super) repository_page: Option<url::Url>,
  pub(super) jobsets:         Vec<Jobset>,
  pub(super) recent_evals:    Vec<EvalView>,
  pub(super) project_mutable: bool,
  pub(super) is_admin:        bool,
  pub(super) auth_name:       String,
  pub(super) csrf_token:      String,
}

pub(super) struct AgentView {
  pub(super) machine_id:       Uuid,
  pub(super) name:             String,
  pub(super) hostname:         String,
  pub(super) systems:          String,
  pub(super) max_jobs:         i32,
  pub(super) current_jobs:     i32,
  pub(super) connected:        bool,
  pub(super) builds_succeeded: i64,
  pub(super) builds_failed:    i64,
  pub(super) last_seen:        String,
  pub(super) last_seen_sort:   i64,
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

pub(super) struct NotificationTaskView {
  pub(super) id:                Uuid,
  pub(super) notification_type: String,
  pub(super) status:            String,
  pub(super) attempts:          i32,
  pub(super) max_attempts:      i32,
  pub(super) next_retry_at:     String,
  pub(super) last_error:        String,
  pub(super) created_at:        String,
}

pub(super) struct PinnedOutputView {
  pub(super) build_id:           Uuid,
  pub(super) product_id:         Uuid,
  pub(super) job_name:           String,
  pub(super) system:             String,
  pub(super) status:             String,
  pub(super) product_name:       String,
  pub(super) path:               String,
  pub(super) gc_root_path:       String,
  pub(super) product_created_at: String,
}

#[cfg(test)]
mod tests {
  use circus_common::models::BinaryCacheUpstreams;
  use circus_config::UiConfig;
  use jiff::Timestamp;

  use super::*;

  #[test]
  fn declarative_project_hides_mutation_controls() {
    let html = ProjectTemplate {
      ui:              UiTemplateConfig::from_config(&UiConfig::default()),
      repository_page: None,
      project:         Project {
        id:                     Uuid::nil(),
        name:                   "declarative".into(),
        description:            None,
        repository_url:         "https://example.com/project".into(),
        cache_enabled:          false,
        cache_url:              None,
        cache_upstreams:        BinaryCacheUpstreams::default(),
        managed_declaratively:  true,
        allow_runtime_mutation: Some(false),
        created_at:             Timestamp::now(),
        updated_at:             Timestamp::now(),
      },
      jobsets:         Vec::new(),
      recent_evals:    Vec::new(),
      project_mutable: false,
      is_admin:        true,
      auth_name:       "admin".into(),
      csrf_token:      "csrf".into(),
    }
    .render()
    .expect("render project template");

    assert!(html.contains("Managed by declarative configuration"));
    assert!(html.contains(">Notifications</a>"));
    assert!(!html.contains("Add Jobset"));
    assert!(!html.contains("Delete Project"));
    assert!(!html.contains("Delete Jobset"));
  }
}

#[derive(Template)]
#[template(path = "admin.html")]
#[expect(
  clippy::struct_excessive_bools,
  reason = "independent template render flags, not a state machine"
)]
pub(super) struct AdminTemplate {
  pub(super) ui:                      UiTemplateConfig,
  pub(super) status:                  SystemStatus,
  pub(super) agents:                  Vec<AgentView>,
  pub(super) agent_sort_headers:      Vec<SortHeaderView>,
  pub(super) agent_sort_key:          String,
  pub(super) agent_sort_dir:          String,
  pub(super) api_keys:                Vec<ApiKeyView>,
  pub(super) notification_tasks:      Vec<NotificationTaskView>,
  pub(super) pinned_outputs:          Vec<PinnedOutputView>,
  pub(super) config_path:             String,
  pub(super) config_contents:         String,
  pub(super) config_editable:         bool,
  pub(super) config_read_only_reason: String,
  pub(super) gc_enabled:              bool,
  pub(super) gc_requested:            bool,
  pub(super) is_admin:                bool,
  pub(super) auth_name:               String,
  pub(super) csrf_token:              String,
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
#[template(path = "users.html")]
pub(super) struct UsersTemplate {
  pub(super) ui:          UiTemplateConfig,
  pub(super) users:       Vec<UserView>,
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

#[derive(Template)]
#[template(path = "metrics.html")]
pub(super) struct MetricsTemplate {
  pub(super) ui:        UiTemplateConfig,
  pub(super) is_admin:  bool,
  pub(super) auth_name: String,
}

#[derive(Template)]
#[template(path = "notifications.html")]
pub(super) struct NotificationsTemplate {
  pub(super) ui:              UiTemplateConfig,
  pub(super) project:         Project,
  pub(super) configs:         Vec<circus_common::models::NotificationConfig>,
  pub(super) project_mutable: bool,
  pub(super) is_admin:        bool,
  pub(super) auth_name:       String,
  pub(super) csrf_token:      String,
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
