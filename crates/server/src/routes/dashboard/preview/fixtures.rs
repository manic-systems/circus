use chrono::{Duration, Utc};
use circus_common::models::{
  BinaryCacheUpstreams,
  Channel,
  Jobset,
  JobsetState,
  JobsetTriggerMode,
  NewsItem,
  Project,
};
use circus_config::UiConfig;
use sqlx::types::Json as SqlxJson;
use uuid::Uuid;

use super::super::{
  shared::{
    BuildErrorLine,
    BuildView,
    EvalSummaryView,
    EvalView,
    JobStatusCell,
    JobStatusColumn,
    JobStatusRow,
    ProjectSummaryView,
    QueueBuildView,
    short_uuid,
  },
  templates::UiTemplateConfig,
};
use crate::permissions::UiPermissions;

pub(super) const fn id(n: u128) -> Uuid {
  Uuid::from_u128(n)
}

pub(super) fn ui() -> UiTemplateConfig {
  let config = UiConfig {
    brand_name: "Circus Preview".into(),
    brand_subtitle: "Fixture-backed frontend".into(),
    ..UiConfig::default()
  };
  UiTemplateConfig::from_config(&config)
}

pub(super) fn csrf() -> String {
  "preview-csrf-token".into()
}

pub(super) const fn permissions() -> UiPermissions {
  UiPermissions {
    admin:           true,
    bump_to_front:   true,
    cancel_build:    true,
    restart_jobs:    true,
    create_projects: true,
    eval_jobset:     true,
  }
}

pub(super) fn project_fixture() -> Project {
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

pub(super) fn jobset_fixture() -> Jobset {
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

pub(super) fn channel_fixture() -> Channel {
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

pub(super) fn news_items() -> Vec<NewsItem> {
  vec![NewsItem {
    id:         id(21),
    title:      "Preview fixtures updated".into(),
    content:    "Frontend previews are served from xtask without a VM.".into(),
    created_by: Some(id(51)),
    created_at: Utc::now() - Duration::hours(2),
  }]
}

pub(super) fn build_view(
  n: u128,
  job: &str,
  status: &str,
  class: &str,
) -> BuildView {
  let build_id = id(n);
  BuildView {
    id:            build_id,
    id_short:      short_uuid(build_id),
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

pub(super) fn builds_fixture() -> Vec<BuildView> {
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

pub(super) fn queue_build(
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

pub(super) fn eval_view(n: u128, status: &str, class: &str) -> EvalView {
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

pub(super) fn evals_fixture() -> Vec<EvalView> {
  vec![
    eval_view(3, "Completed", "completed"),
    eval_view(13, "Running", "running"),
  ]
}

pub(super) fn eval_summaries() -> Vec<EvalSummaryView> {
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

pub(super) fn project_summaries() -> Vec<ProjectSummaryView> {
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

pub(super) fn job_columns() -> Vec<JobStatusColumn> {
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

pub(super) fn job_rows() -> Vec<JobStatusRow> {
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
