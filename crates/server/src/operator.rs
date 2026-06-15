use std::collections::HashMap;

use chrono::Utc;
use circus_common::models::{Build, BuildStatus, Evaluation};
use serde::Serialize;
use uuid::Uuid;

use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct OperatorOverview {
  pub total_builds:       i64,
  pub completed_builds:   i64,
  pub failed_builds:      i64,
  pub running_builds:     i64,
  pub pending_builds:     i64,
  pub recent_builds:      Vec<OperatorBuild>,
  pub failed_builds_list: Vec<OperatorBuild>,
  pub projects:           Vec<OperatorProject>,
  pub queue_by_system:    Vec<OperatorQueueSystem>,
  pub workers:            Vec<OperatorWorker>,
  pub worker_online:      i64,
  pub worker_total:       i64,
  pub refreshed_at:       String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperatorBuild {
  pub id:            Uuid,
  pub job_name:      String,
  pub project_id:    Option<Uuid>,
  pub project_name:  String,
  pub jobset_id:     Option<Uuid>,
  pub jobset_name:   String,
  pub status:        String,
  pub status_text:   String,
  pub status_class:  String,
  pub system:        String,
  pub created_at:    String,
  pub started_at:    String,
  pub completed_at:  String,
  pub duration:      String,
  pub started_epoch: Option<i64>,
  pub priority:      i32,
  pub is_aggregate:  bool,
  pub signed:        bool,
  pub drv_path:      String,
  pub output_path:   String,
  pub error_message: String,
  pub has_log:       bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperatorProject {
  pub id:               Uuid,
  pub name:             String,
  pub jobset_count:     i64,
  pub last_eval_status: String,
  pub last_eval_class:  String,
  pub last_eval_time:   String,
  pub failing_jobs:     i64,
  pub queued_jobs:      i64,
  pub systems:          String,
  pub updated_at:       String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperatorQueueSystem {
  pub system: String,
  pub count:  i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperatorWorker {
  pub name:         String,
  pub system:       String,
  pub status_text:  String,
  pub status_class: String,
  pub current_jobs: i32,
  pub max_jobs:     i32,
}

pub async fn overview(
  state: &AppState,
  include_hidden: bool,
) -> OperatorOverview {
  let build_stats = circus_common::repo::builds::get_stats(&state.pool)
    .await
    .unwrap_or_default();
  let recent_raw = circus_common::repo::builds::list_recent(&state.pool, 40)
    .await
    .unwrap_or_default();
  let failed_raw = circus_common::repo::builds::list_filtered(
    &state.pool,
    None,
    Some("failed"),
    None,
    None,
    12,
    0,
  )
  .await
  .unwrap_or_default();
  let pending_raw =
    circus_common::repo::builds::list_pending_in_scheduler_order(
      &state.pool,
      100,
      0,
    )
    .await
    .unwrap_or_default();

  let context_by_eval =
    context_for_builds(state, recent_raw.iter().chain(failed_raw.iter())).await;
  let recent_builds = recent_raw
    .iter()
    .map(|b| operator_build(b, context_by_eval.get(&b.evaluation_id)))
    .collect();
  let failed_builds_list = failed_raw
    .iter()
    .map(|b| operator_build(b, context_by_eval.get(&b.evaluation_id)))
    .collect();

  let projects = operator_projects(state, include_hidden).await;
  let queue_by_system = queue_by_system(&pending_raw);
  let (workers, worker_online, worker_total) = workers(state).await;

  OperatorOverview {
    total_builds: build_stats.total_builds.unwrap_or(0),
    completed_builds: build_stats.completed_builds.unwrap_or(0),
    failed_builds: build_stats.failed_builds.unwrap_or(0),
    running_builds: build_stats.running_builds.unwrap_or(0),
    pending_builds: build_stats.pending_builds.unwrap_or(0),
    recent_builds,
    failed_builds_list,
    projects,
    queue_by_system,
    workers,
    worker_online,
    worker_total,
    refreshed_at: Utc::now().format("%H:%M UTC").to_string(),
  }
}

pub async fn recent_builds(state: &AppState) -> Vec<OperatorBuild> {
  let builds = circus_common::repo::builds::list_recent(&state.pool, 40)
    .await
    .unwrap_or_default();
  let context_by_eval = context_for_builds(state, builds.iter()).await;
  builds
    .iter()
    .map(|b| operator_build(b, context_by_eval.get(&b.evaluation_id)))
    .collect()
}

pub async fn failures(state: &AppState) -> Vec<OperatorBuild> {
  let builds = circus_common::repo::builds::list_filtered(
    &state.pool,
    None,
    Some("failed"),
    None,
    None,
    50,
    0,
  )
  .await
  .unwrap_or_default();
  let context_by_eval = context_for_builds(state, builds.iter()).await;
  builds
    .iter()
    .map(|b| operator_build(b, context_by_eval.get(&b.evaluation_id)))
    .collect()
}

pub async fn projects(
  state: &AppState,
  include_hidden: bool,
) -> Vec<OperatorProject> {
  operator_projects(state, include_hidden).await
}

pub async fn queue(state: &AppState) -> Vec<OperatorQueueSystem> {
  let pending = circus_common::repo::builds::list_pending_in_scheduler_order(
    &state.pool,
    100,
    0,
  )
  .await
  .unwrap_or_default();
  queue_by_system(&pending)
}

pub async fn worker_summary(state: &AppState) -> Vec<OperatorWorker> {
  let (workers, ..) = workers(state).await;
  workers
}

type BuildContext = (Uuid, String, Uuid, String);

async fn context_for_builds<'a>(
  state: &AppState,
  builds: impl Iterator<Item = &'a Build>,
) -> HashMap<Uuid, BuildContext> {
  let mut context_by_eval = HashMap::new();
  for item in builds {
    if context_by_eval.contains_key(&item.evaluation_id) {
      continue;
    }
    if let Some(context) =
      context_for_evaluation(state, item.evaluation_id).await
    {
      context_by_eval.insert(item.evaluation_id, context);
    }
  }
  context_by_eval
}

async fn context_for_evaluation(
  state: &AppState,
  evaluation_id: Uuid,
) -> Option<BuildContext> {
  let eval = circus_common::repo::evaluations::get(&state.pool, evaluation_id)
    .await
    .ok()?;
  let jobset = circus_common::repo::jobsets::get(&state.pool, eval.jobset_id)
    .await
    .ok()?;
  let project =
    circus_common::repo::projects::get(&state.pool, jobset.project_id)
      .await
      .ok()?;
  Some((project.id, project.name, jobset.id, jobset.name))
}

fn operator_build(b: &Build, context: Option<&BuildContext>) -> OperatorBuild {
  let (status_text, status_class) = status_badge(b.status);
  let (project_id, project_name, jobset_id, jobset_name) = context.map_or_else(
    || (None, String::new(), None, String::new()),
    |(project_id, project_name, jobset_id, jobset_name)| {
      (
        Some(*project_id),
        project_name.clone(),
        Some(*jobset_id),
        jobset_name.clone(),
      )
    },
  );
  OperatorBuild {
    id: b.id,
    job_name: b.job_name.clone(),
    project_id,
    project_name,
    jobset_id,
    jobset_name,
    status: format!("{:?}", b.status),
    status_text,
    status_class,
    system: b.system.clone().unwrap_or_else(|| "-".to_string()),
    created_at: b.created_at.format("%Y-%m-%d %H:%M").to_string(),
    started_at: b
      .started_at
      .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
      .unwrap_or_default(),
    completed_at: b
      .completed_at
      .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string())
      .unwrap_or_default(),
    duration: format_duration(b.started_at.as_ref(), b.completed_at.as_ref()),
    started_epoch: if b.completed_at.is_none() {
      b.started_at.map(|t| t.timestamp())
    } else {
      None
    },
    priority: b.priority,
    is_aggregate: b.is_aggregate,
    signed: b.signed,
    drv_path: b.drv_path.clone(),
    output_path: b.build_output_path.clone().unwrap_or_default(),
    error_message: b.error_message.clone().unwrap_or_default(),
    has_log: b.log_path.as_deref().is_some_and(|p| !p.is_empty()),
  }
}

async fn operator_projects(
  state: &AppState,
  include_hidden: bool,
) -> Vec<OperatorProject> {
  let all_projects = circus_common::repo::projects::list(&state.pool, 100, 0)
    .await
    .unwrap_or_default();
  let mut project_summaries = Vec::new();
  for p in &all_projects {
    let jobset_count =
      circus_common::repo::jobsets::count_for_project(&state.pool, p.id)
        .await
        .unwrap_or(0);
    let jobsets =
      circus_common::repo::jobsets::list_for_project(&state.pool, p.id, 100, 0)
        .await
        .unwrap_or_default();
    let mut last_eval: Option<Evaluation> = None;
    for js in &jobsets {
      let js_evals =
        circus_common::repo::evaluations::list_filtered_with_visibility(
          &state.pool,
          Some(js.id),
          None,
          1,
          0,
          include_hidden,
        )
        .await
        .unwrap_or_default();
      if let Some(e) = js_evals.into_iter().next()
        && last_eval
          .as_ref()
          .is_none_or(|le| e.evaluation_time > le.evaluation_time)
      {
        last_eval = Some(e);
      }
    }
    let (last_eval_status, last_eval_class, last_eval_time) =
      last_eval.as_ref().map_or_else(
        || ("-".into(), "pending".into(), "-".into()),
        |e| {
          let (text, class) = eval_badge(&e.status);
          (
            text,
            class,
            e.evaluation_time.format("%Y-%m-%d %H:%M").to_string(),
          )
        },
      );
    let project_builds =
      circus_common::repo::builds::list_for_project(&state.pool, p.id)
        .await
        .unwrap_or_default();
    let failing_jobs = project_builds
      .iter()
      .filter(|b| is_failed_status(b.status))
      .count() as i64;
    let queued_jobs = project_builds
      .iter()
      .filter(|b| b.status == BuildStatus::Pending)
      .count() as i64;
    let mut systems = project_builds
      .iter()
      .filter_map(|b| b.system.clone())
      .collect::<Vec<_>>();
    systems.sort();
    systems.dedup();
    project_summaries.push(OperatorProject {
      id: p.id,
      name: p.name.clone(),
      jobset_count,
      last_eval_status,
      last_eval_class,
      last_eval_time,
      failing_jobs,
      queued_jobs,
      systems: if systems.is_empty() {
        "-".into()
      } else {
        systems.join(", ")
      },
      updated_at: p.updated_at.format("%Y-%m-%d %H:%M").to_string(),
    });
  }
  project_summaries
}

fn queue_by_system(pending: &[Build]) -> Vec<OperatorQueueSystem> {
  let mut queue_counts: HashMap<String, i64> = HashMap::new();
  for build in pending {
    let system = build
      .system
      .clone()
      .unwrap_or_else(|| "unknown".to_string());
    *queue_counts.entry(system).or_default() += 1;
  }
  let canonical_systems = [
    "x86_64-linux",
    "aarch64-linux",
    "aarch64-darwin",
    "x86_64-darwin",
  ];
  canonical_systems
    .iter()
    .filter_map(|system| {
      queue_counts.get(*system).map(|count| {
        OperatorQueueSystem {
          system: (*system).to_string(),
          count:  *count,
        }
      })
    })
    .chain(
      queue_counts
        .iter()
        .filter(|(system, _)| !canonical_systems.contains(&system.as_str()))
        .map(|(system, count)| {
          OperatorQueueSystem {
            system: system.clone(),
            count:  *count,
          }
        }),
    )
    .collect()
}

async fn workers(state: &AppState) -> (Vec<OperatorWorker>, i64, i64) {
  let worker_sessions =
    circus_common::repo::builder_sessions::list(&state.pool)
      .await
      .unwrap_or_default();
  let worker_total = worker_sessions.len() as i64;
  let worker_online =
    worker_sessions.iter().filter(|w| w.connected).count() as i64;
  let workers = worker_sessions
    .iter()
    .take(8)
    .map(|w| {
      let status_text = if !w.connected {
        "offline"
      } else if w.current_jobs > 0 {
        "busy"
      } else {
        "idle"
      };
      OperatorWorker {
        name:         w.name.clone(),
        system:       if w.systems.is_empty() {
          "unknown".into()
        } else {
          w.systems.join(", ")
        },
        status_text:  status_text.into(),
        status_class: if w.connected {
          "running".into()
        } else {
          "skipped".into()
        },
        current_jobs: w.current_jobs,
        max_jobs:     w.max_jobs,
      }
    })
    .collect();
  (workers, worker_online, worker_total)
}

fn format_duration(
  started: Option<&chrono::DateTime<chrono::Utc>>,
  completed: Option<&chrono::DateTime<chrono::Utc>>,
) -> String {
  match (started, completed) {
    (Some(s), Some(c)) => {
      let secs = (*c - *s).num_seconds();
      if secs < 0 {
        return String::new();
      }
      let mins = secs / 60;
      let rem = secs % 60;
      if mins > 0 {
        format!("{mins}m {rem}s")
      } else {
        format!("{rem}s")
      }
    },
    _ => String::new(),
  }
}

fn is_failed_status(status: BuildStatus) -> bool {
  matches!(
    status,
    BuildStatus::Failed
      | BuildStatus::DependencyFailed
      | BuildStatus::FailedWithOutput
      | BuildStatus::Timeout
      | BuildStatus::CachedFailure
      | BuildStatus::LogLimitExceeded
      | BuildStatus::NarSizeLimitExceeded
      | BuildStatus::NonDeterministic
  )
}

fn status_badge(s: BuildStatus) -> (String, String) {
  match s {
    BuildStatus::Succeeded => ("Succeeded".into(), "succeeded".into()),
    BuildStatus::Failed => ("Failed".into(), "failed".into()),
    BuildStatus::Running => ("Running".into(), "running".into()),
    BuildStatus::Pending => ("Pending".into(), "pending".into()),
    BuildStatus::Cancelled => ("Cancelled".into(), "cancelled".into()),
    BuildStatus::DependencyFailed => {
      ("Dependency Failed".into(), "failed".into())
    },
    BuildStatus::Aborted => ("Aborted".into(), "aborted".into()),
    BuildStatus::FailedWithOutput => {
      ("Failed w/ Output".into(), "failed".into())
    },
    BuildStatus::Timeout => ("Timeout".into(), "failed".into()),
    BuildStatus::CachedFailure => ("Cached Failure".into(), "failed".into()),
    BuildStatus::UnsupportedSystem => {
      ("Unsupported System".into(), "skipped".into())
    },
    BuildStatus::LogLimitExceeded => ("Log Limit".into(), "failed".into()),
    BuildStatus::NarSizeLimitExceeded => {
      ("NAR Size Limit".into(), "failed".into())
    },
    BuildStatus::NonDeterministic => {
      ("Non-deterministic".into(), "failed".into())
    },
  }
}

fn eval_badge(s: &circus_common::models::EvaluationStatus) -> (String, String) {
  match s {
    circus_common::models::EvaluationStatus::Completed => {
      ("Completed".into(), "completed".into())
    },
    circus_common::models::EvaluationStatus::Failed => {
      ("Failed".into(), "failed".into())
    },
    circus_common::models::EvaluationStatus::Running => {
      ("Running".into(), "running".into())
    },
    circus_common::models::EvaluationStatus::Pending => {
      ("Pending".into(), "pending".into())
    },
  }
}
