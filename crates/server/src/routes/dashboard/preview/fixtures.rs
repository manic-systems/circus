use circus_common::models::{
  BinaryCacheUpstreams,
  Jobset,
  JobsetState,
  JobsetTriggerMode,
  Project,
};
use circus_config::UiConfig;
use jiff::{SignedDuration, Timestamp};
use uuid::Uuid;

use super::super::{
  shared::{EvalProgressView, EvalView},
  templates::UiTemplateConfig,
};

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

pub(super) fn project_fixture() -> Project {
  Project {
    id:                     id(1),
    name:                   "circus".into(),
    description:            Some("Nix-native CI control plane".into()),
    repository_url:         "https://github.com/manic-systems/circus".into(),
    cache_enabled:          true,
    cache_url:              Some("https://cache.example.invalid".into()),
    cache_upstreams:        BinaryCacheUpstreams::default(),
    managed_declaratively:  false,
    allow_runtime_mutation: None,
    created_at:             Timestamp::now()
      - SignedDuration::from_hours(24 * 30),
    updated_at:             Timestamp::now() - SignedDuration::from_mins(5),
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
    created_at:        Timestamp::now() - SignedDuration::from_hours(24 * 20),
    updated_at:        Timestamp::now() - SignedDuration::from_mins(5),
    state:             JobsetState::Enabled,
    last_checked_at:   Some(Timestamp::now() - SignedDuration::from_mins(10)),
    keep_nr:           3,
    systems:           None,
    only_build_latest: false,
    path_filters:      Vec::new(),
  }
}

pub(super) fn eval_view(n: u128, status: &str, class: &str) -> EvalView {
  EvalView {
    commit_url:     Some(
      "https://github.com/manic-systems/circus/commit/9f2c7a113badf00d7e57c0ffee1234567890abcd"
        .into(),
    ),
    id:             id(n),
    commit_hash:    "9f2c7a113badf00d7e57c0ffee1234567890abcd".into(),
    commit_short:   "9f2c7a113bad".into(),
    commit_subject: "evaluator: record the commit subject".into(),
    status_text:    status.into(),
    status_class:   class.into(),
    time:           "2026-06-18 11:42 UTC".into(),
    time_iso:       "2026-06-18T11:42:00+00:00".into(),
    started:        "2026-06-18 11:42 UTC".into(),
    started_iso:    "2026-06-18T11:42:00+00:00".into(),
    finished:       if status == "Running" {
      "-".into()
    } else {
      "2026-06-18 11:44 UTC".into()
    },
    finished_iso:   if status == "Running" {
      String::new()
    } else {
      "2026-06-18T11:44:00+00:00".into()
    },
    duration:       if status == "Running" {
      String::new()
    } else {
      "1m 12s".into()
    },
    running_since:  (status == "Running").then(|| Timestamp::now().as_second() - 40),
    progress:       (status == "Running").then(|| {
      EvalProgressView {
        count:   "1,204 / 1,530".into(),
        percent: 78,
      }
    }),
    error_message:  String::new(),
    error_segments: Vec::new(),
    hidden:         false,
    superseded_by:  None,
    jobset_name:    "packages".into(),
    project_name:   "circus".into(),
  }
}

pub(super) fn evals_fixture() -> Vec<EvalView> {
  vec![
    eval_view(3, "Completed", "completed"),
    eval_view(13, "Running", "running"),
  ]
}
