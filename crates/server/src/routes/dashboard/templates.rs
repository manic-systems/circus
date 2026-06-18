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
use circus_config::UiConfig;
use uuid::Uuid;

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

#[derive(Clone)]
pub(super) struct UiTemplateConfig {
  pub(super) brand_name:     String,
  pub(super) brand_subtitle: String,
  pub(super) logo_url:       String,
  pub(super) has_logo:       bool,
  pub(super) favicon_url:    String,
  pub(super) has_favicon:    bool,
  pub(super) has_custom_css: bool,
}

impl UiTemplateConfig {
  pub(super) fn from_config(config: &UiConfig) -> Self {
    let logo_url = config.logo_url.clone().unwrap_or_default();
    let favicon_url = config.favicon_url.clone().unwrap_or_default();
    Self {
      brand_name: config.brand_name.clone(),
      brand_subtitle: config.brand_subtitle.clone(),
      has_logo: !logo_url.is_empty(),
      logo_url,
      has_favicon: !favicon_url.is_empty(),
      favicon_url,
      has_custom_css: config.custom_css.is_some(),
    }
  }
}

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
  pub(super) channels:  Vec<Channel>,
  pub(super) is_admin:  bool,
  pub(super) auth_name: String,
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
  use super::*;

  fn build(
    id: Uuid,
    job_name: &str,
    status_text: &str,
    status_class: &str,
  ) -> BuildView {
    BuildView {
      id,
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
      worker_online: 1,
      worker_total: 1,
      refreshed_at: "09:27 UTC".into(),
      announcements: Vec::new(),
      is_admin: true,
      auth_name: "operator".into(),
    }
  }

  #[test]
  fn dashboard_renders_operator_console_with_failures_first() {
    let html = dashboard(
      vec![build(Uuid::nil(), "checks.default", "Failed", "failed")],
      vec![build(Uuid::nil(), "checks.default", "Failed", "failed")],
    )
    .render()
    .expect("render dashboard");
    assert!(html.contains("Dashboard"));
    assert!(html.contains("Failures"));
    assert!(html.contains("/builds?status=failed"));
    assert!(html.contains("status-failed"));
    assert!(html.contains("data-table dense-table"));
    assert!(html.contains("metric-strip"));
    assert!(!html.contains("stat-card"));
  }

  #[test]
  fn dashboard_renders_empty_states_without_database() {
    let html = dashboard(Vec::new(), Vec::new())
      .render()
      .expect("render empty dashboard");
    assert!(html.contains("No builds yet"));
    assert!(html.contains("No failed builds"));
    assert!(html.contains("filter project, job, system"));
  }

  #[test]
  fn dashboard_long_names_have_titles_for_truncation() {
    let html = dashboard(
      vec![build(
        Uuid::nil(),
        "very.long.job.name.with.many.components.default",
        "Running",
        "running",
      )],
      Vec::new(),
    )
    .render()
    .expect("render long names");
    assert!(html.contains("class=\"truncate\""));
    assert!(
      html
        .contains("title=\"very.long.job.name.with.many.components.default\"")
    );
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
