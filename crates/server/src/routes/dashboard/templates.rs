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

use super::shared::{
  ApiKeyView,
  BuildView,
  EvalSummaryView,
  EvalView,
  JobStatusColumn,
  JobStatusRow,
  ProjectSummaryView,
  QueueBuildView,
  StarredJobView,
  UserView,
};
use crate::permissions::UiPermissions;

#[derive(Template)]
#[template(path = "home.html")]
pub(super) struct HomeTemplate {
  pub(super) total_builds:     i64,
  pub(super) completed_builds: i64,
  pub(super) failed_builds:    i64,
  pub(super) running_builds:   i64,
  pub(super) pending_builds:   i64,
  pub(super) recent_builds:    Vec<BuildView>,
  pub(super) recent_evals:     Vec<EvalView>,
  pub(super) projects:         Vec<ProjectSummaryView>,
  pub(super) announcements:    Vec<NewsItem>,
  pub(super) is_admin:         bool,
  pub(super) auth_name:        String,
}

#[derive(Template)]
#[template(path = "projects.html")]
pub(super) struct ProjectsTemplate {
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
  pub(super) channels:  Vec<Channel>,
  pub(super) is_admin:  bool,
  pub(super) auth_name: String,
}

#[derive(Template)]
#[template(path = "channel.html")]
pub(super) struct ChannelTemplate {
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

#[derive(Template)]
#[template(path = "admin.html")]
pub(super) struct AdminTemplate {
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
  pub(super) is_admin:   bool,
  pub(super) auth_name:  String,
  pub(super) csrf_token: String,
}

#[derive(Template)]
#[template(path = "login.html")]
pub(super) struct LoginTemplate {
  pub(super) error:     Option<String>,
  pub(super) is_admin:  bool,
  pub(super) auth_name: String,
}

#[derive(Template)]
#[template(path = "users.html")]
pub(super) struct UsersTemplate {
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
  pub(super) starred_jobs: Vec<StarredJobView>,
  pub(super) is_logged_in: bool,
  pub(super) is_admin:     bool,
  pub(super) auth_name:    String,
  pub(super) csrf_token:   String,
}

#[derive(Template)]
#[template(path = "metrics.html")]
pub(super) struct MetricsTemplate {
  pub(super) is_admin:  bool,
  pub(super) auth_name: String,
}

#[derive(Template)]
#[template(path = "notifications.html")]
pub(super) struct NotificationsTemplate {
  pub(super) project:    Project,
  pub(super) configs:    Vec<circus_common::models::NotificationConfig>,
  pub(super) is_admin:   bool,
  pub(super) auth_name:  String,
  pub(super) csrf_token: String,
}
