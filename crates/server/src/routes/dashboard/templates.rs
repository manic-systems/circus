//! Askama template structs for every dashboard page. The structs are
//! field-private from outside this module, but `pub(super)` for sibling
//! handler modules. Each `#[derive(Template)]` macro looks for the
//! `path = "..."` HTML template under the configured templates root.
#![expect(
  dead_code,
  reason = "Askama templates read fields from generated render impls"
)]

use askama::Template;
use circus_common::models::{
  BuildProduct,
  BuildStep,
  Channel,
  Jobset,
  NewsItem,
  Project,
  SystemStatus,
};
use uuid::Uuid;

pub(super) use super::shared::UiTemplateConfig;
use super::shared::{
  ApiKeyView,
  BuildView,
  EvalSummaryView,
  EvalView,
  JobStatusColumn,
  JobStatusRow,
  ProjectSummaryView,
  QueueBuildView,
  QueueSystemView,
  StarredJobView,
  UserView,
  WorkerSummaryView,
};
use crate::permissions::UiPermissions;

#[derive(Template)]
#[template(path = "home.html")]
pub(super) struct HomeTemplate {
  pub(super) ui:                 UiTemplateConfig,
  pub(super) total_builds:       i64,
  pub(super) completed_builds:   i64,
  pub(super) failed_builds:      i64,
  pub(super) running_builds:     i64,
  pub(super) pending_builds:     i64,
  pub(super) recent_builds:      Vec<BuildView>,
  pub(super) failed_builds_list: Vec<BuildView>,
  pub(super) recent_evals:       Vec<EvalView>,
  pub(super) projects:           Vec<ProjectSummaryView>,
  pub(super) queue_by_system:    Vec<QueueSystemView>,
  pub(super) workers:            Vec<WorkerSummaryView>,
  pub(super) system_filters:     Vec<String>,
  pub(super) worker_online:      i64,
  pub(super) worker_total:       i64,
  pub(super) refreshed_at:       String,
  pub(super) announcements:      Vec<NewsItem>,
  pub(super) is_admin:           bool,
  pub(super) auth_name:          String,
}

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
  pub(super) ui:           UiTemplateConfig,
  pub(super) project:      Project,
  pub(super) jobsets:      Vec<Jobset>,
  pub(super) recent_evals: Vec<EvalView>,
  pub(super) is_admin:     bool,
  pub(super) auth_name:    String,
  pub(super) csrf_token:   String,
}

#[derive(Template)]
#[template(path = "jobset.html")]
pub(super) struct JobsetTemplate {
  pub(super) ui:             UiTemplateConfig,
  pub(super) project:        Project,
  pub(super) jobset:         Jobset,
  pub(super) eval_summaries: Vec<EvalSummaryView>,
  pub(super) is_admin:       bool,
  pub(super) auth_name:      String,
  pub(super) csrf_token:     String,
}

#[derive(Template)]
#[template(path = "jobset_jobs.html")]
pub(super) struct JobsetJobsTemplate {
  pub(super) ui:            UiTemplateConfig,
  pub(super) project:       Project,
  pub(super) jobset:        Jobset,
  pub(super) columns:       Vec<JobStatusColumn>,
  pub(super) rows:          Vec<JobStatusRow>,
  pub(super) show_inactive: bool,
  pub(super) is_admin:      bool,
  pub(super) auth_name:     String,
}

#[derive(Template)]
#[template(path = "evaluations.html")]
pub(super) struct EvaluationsTemplate {
  pub(super) ui:          UiTemplateConfig,
  pub(super) evals:       Vec<EvalView>,
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
#[template(path = "evaluation.html")]
pub(super) struct EvaluationTemplate {
  pub(super) ui:              UiTemplateConfig,
  pub(super) eval:            EvalView,
  pub(super) builds:          Vec<BuildView>,
  pub(super) project_name:    String,
  pub(super) project_id:      Uuid,
  pub(super) jobset_name:     String,
  pub(super) jobset_id:       Uuid,
  pub(super) succeeded_count: i64,
  pub(super) failed_count:    i64,
  pub(super) running_count:   i64,
  pub(super) pending_count:   i64,
  pub(super) is_admin:        bool,
  pub(super) auth_name:       String,
  pub(super) csrf_token:      String,
}

#[derive(Template)]
#[template(path = "builds.html")]
pub(super) struct BuildsTemplate {
  pub(super) ui:            UiTemplateConfig,
  pub(super) builds:        Vec<BuildView>,
  pub(super) limit:         i64,
  pub(super) has_prev:      bool,
  pub(super) has_next:      bool,
  pub(super) prev_offset:   i64,
  pub(super) next_offset:   i64,
  pub(super) page:          i64,
  pub(super) total_pages:   i64,
  pub(super) filter_status: String,
  pub(super) filter_system: String,
  pub(super) filter_job:    String,
  pub(super) is_admin:      bool,
  pub(super) auth_name:     String,
}

#[derive(Template)]
#[template(path = "build.html")]
pub(super) struct BuildTemplate {
  pub(super) ui:                UiTemplateConfig,
  pub(super) build:             BuildView,
  pub(super) builder_label:     String,
  pub(super) steps:             Vec<BuildStep>,
  pub(super) products:          Vec<BuildProduct>,
  pub(super) dependencies:      Vec<BuildView>,
  pub(super) dependents:        Vec<BuildView>,
  pub(super) eval_id:           Uuid,
  pub(super) eval_commit_short: String,
  pub(super) jobset_id:         Uuid,
  pub(super) jobset_name:       String,
  pub(super) project_id:        Uuid,
  pub(super) project_name:      String,
  pub(super) is_admin:          bool,
  pub(super) auth_name:         String,
}

#[derive(Template)]
#[template(path = "queue.html")]
pub(super) struct QueueTemplate {
  pub(super) ui:             UiTemplateConfig,
  pub(super) pending_builds: Vec<QueueBuildView>,
  pub(super) running_builds: Vec<QueueBuildView>,
  pub(super) pending_count:  i64,
  pub(super) running_count:  i64,
  pub(super) permissions:    UiPermissions,
  pub(super) csrf_token:     String,
  pub(super) is_admin:       bool,
  pub(super) auth_name:      String,
}

#[derive(Template)]
#[template(path = "channels.html")]
pub(super) struct ChannelsTemplate {
  pub(super) ui:        UiTemplateConfig,
  pub(super) channels:  Vec<ChannelView>,
  pub(super) is_admin:  bool,
  pub(super) auth_name: String,
}

pub(super) struct ChannelView {
  pub(super) id:                    Uuid,
  pub(super) name:                  String,
  pub(super) current_evaluation_id: Option<Uuid>,
  pub(super) updated_at:            String,
  pub(super) status_text:           String,
  pub(super) status_class:          String,
  pub(super) job_count:             i64,
}

#[derive(Template)]
#[template(path = "channel.html")]
pub(super) struct ChannelTemplate {
  pub(super) ui:              UiTemplateConfig,
  pub(super) channel:         Channel,
  pub(super) builds:          Vec<BuildView>,
  pub(super) succeeded_count: i64,
  pub(super) failed_count:    i64,
  pub(super) pending_count:   i64,
  pub(super) is_admin:        bool,
  pub(super) auth_name:       String,
}

#[derive(Template)]
#[template(path = "news.html")]
pub(super) struct NewsTemplate {
  pub(super) ui:         UiTemplateConfig,
  pub(super) items:      Vec<NewsItem>,
  pub(super) is_admin:   bool,
  pub(super) auth_name:  String,
  pub(super) csrf_token: String,
}

/// Builder info with load and activity metrics
pub(super) struct BuilderView {
  pub(super) id:             Uuid,
  pub(super) name:           String,
  pub(super) ssh_uri:        String,
  pub(super) systems:        String,
  pub(super) max_jobs:       i32,
  pub(super) enabled:        bool,
  pub(super) current_builds: i64,
  pub(super) load_percent:   i64,
  pub(super) last_activity:  String,
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
  use circus_config::UiConfig;

  use super::*;

  fn build(
    id: Uuid,
    job_name: &str,
    status_text: &str,
    status_class: &str,
  ) -> BuildView {
    BuildView {
      id,
      id_short: super::super::shared::short_uuid(id),
      job_name: job_name.into(),
      project_id: Some(Uuid::nil()),
      project_name: "nh".into(),
      jobset_id: Some(Uuid::nil()),
      jobset_name: "default".into(),
      status_text: status_text.into(),
      status_class: status_class.into(),
      system: "aarch64-linux".into(),
      created_at: "2026-06-15 09:27".into(),
      started_at: String::new(),
      completed_at: String::new(),
      duration: "1m 12s".into(),
      started_epoch: None,
      priority: 0,
      is_aggregate: false,
      signed: false,
      drv_path: "/nix/store/very-long-derivation-path-that-should-truncate.drv"
        .into(),
      output_path: "/nix/store/very-long-output-path-that-should-truncate"
        .into(),
      error_message: String::new(),
      error_lines: Vec::new(),
      has_log: true,
    }
  }

  fn dashboard(
    recent_builds: Vec<BuildView>,
    failed_builds_list: Vec<BuildView>,
  ) -> HomeTemplate {
    HomeTemplate {
      ui: UiTemplateConfig::from_config(&UiConfig::default()),
      total_builds: 1859,
      completed_builds: 1480,
      failed_builds: 272,
      running_builds: 1,
      pending_builds: 70,
      recent_builds,
      failed_builds_list,
      recent_evals: Vec::new(),
      projects: vec![ProjectSummaryView {
        id:               Uuid::nil(),
        name:
          "very-long-project-name-that-needs-predictable-truncation".into(),
        jobset_count:     2,
        last_eval_status: "Succeeded".into(),
        last_eval_class:  "completed".into(),
        last_eval_time:   "2026-06-15 09:20".into(),
        failing_jobs:     3,
        queued_jobs:      5,
        systems:          "x86_64-linux, aarch64-linux".into(),
        updated_at:       "2026-06-15 09:21".into(),
      }],
      queue_by_system: vec![QueueSystemView {
        system: "aarch64-linux".into(),
        count:  70,
      }],
      workers: vec![WorkerSummaryView {
        name:         "builder-01".into(),
        system:       "aarch64-linux".into(),
        status_text:  "busy".into(),
        status_class: "running".into(),
        current_jobs: 1,
        max_jobs:     4,
      }],
      system_filters: vec!["aarch64-linux".into(), "x86_64-linux".into()],
      worker_online: 1,
      worker_total: 1,
      refreshed_at: "09:27 UTC".into(),
      announcements: Vec::new(),
      is_admin: true,
      auth_name: "operator".into(),
    }
  }
}

#[derive(Template)]
#[template(path = "admin.html")]
pub(super) struct AdminTemplate {
  pub(super) ui:                      UiTemplateConfig,
  pub(super) status:                  SystemStatus,
  pub(super) builders:                Vec<BuilderView>,
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
#[template(path = "login.html")]
pub(super) struct LoginTemplate {
  pub(super) ui:        UiTemplateConfig,
  pub(super) error:     Option<String>,
  pub(super) is_admin:  bool,
  pub(super) auth_name: String,
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
#[template(path = "starred.html")]
pub(super) struct StarredTemplate {
  pub(super) ui:           UiTemplateConfig,
  pub(super) starred_jobs: Vec<StarredJobView>,
  pub(super) is_logged_in: bool,
  pub(super) is_admin:     bool,
  pub(super) auth_name:    String,
  pub(super) csrf_token:   String,
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
  pub(super) ui:         UiTemplateConfig,
  pub(super) project:    Project,
  pub(super) configs:    Vec<circus_common::models::NotificationConfig>,
  pub(super) is_admin:   bool,
  pub(super) auth_name:  String,
  pub(super) csrf_token: String,
}

/// One row in the unified Caches listing.
pub(super) struct CacheRowView {
  pub(super) name:              String,
  pub(super) scope_label:       String,
  pub(super) active:            bool,
  pub(super) nar_count:         i64,
  pub(super) compressed:        String,
  pub(super) requests_per_hour: i64,
  pub(super) detail_href:       String,
}

#[derive(Template)]
#[template(path = "caches.html")]
pub(super) struct CachesTemplate {
  pub(super) ui:                 UiTemplateConfig,
  pub(super) is_admin:           bool,
  pub(super) auth_name:          String,
  pub(super) total_nars:         i64,
  pub(super) total_compressed:   String,
  pub(super) total_uncompressed: String,
  pub(super) caches:             Vec<CacheRowView>,
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
}

/// One row in the per-cache NAR inventory.
pub(super) struct NarRowView {
  pub(super) hash:         String,
  pub(super) package:      String,
  pub(super) store_path:   String,
  pub(super) nar_size:     String,
  pub(super) compressed:   String,
  pub(super) created_at:   String,
  pub(super) last_fetched: String,
}

#[derive(Template)]
#[template(path = "cache_nars.html")]
pub(super) struct CacheNarsTemplate {
  pub(super) ui:             UiTemplateConfig,
  pub(super) is_admin:       bool,
  pub(super) auth_name:      String,
  pub(super) name:           String,
  pub(super) scope_label:    String,
  pub(super) detail_href:    String,
  pub(super) filter_hash:    String,
  pub(super) filter_package: String,
  pub(super) total_nars:     i64,
  pub(super) nar_size:       String,
  pub(super) file_size:      String,
  pub(super) last_uploaded:  String,
  pub(super) oldest_fetched: String,
  pub(super) nars:           Vec<NarRowView>,
  pub(super) page:           i64,
  pub(super) total_pages:    i64,
  pub(super) has_prev:       bool,
  pub(super) has_next:       bool,
  pub(super) prev_offset:    i64,
  pub(super) next_offset:    i64,
  pub(super) limit:          i64,
}
