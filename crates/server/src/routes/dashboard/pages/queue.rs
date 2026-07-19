use std::collections::HashMap;

use axum::{
  extract::{Query, State},
  response::{Html, Response},
};
use circus_common::models::Build;
use uuid::Uuid;

use super::{
  super::{
    shared::{
      DashboardContext,
      DashboardPage,
      QueueBuildView,
      RenderExt,
      enforce_page_access,
    },
    templates::QueueTemplate,
  },
  format_elapsed,
  ui_config,
};
use crate::state::AppState;

#[derive(serde::Deserialize)]
pub(in crate::routes::dashboard) struct QueueFilterParams {
  #[serde(
    default,
    deserialize_with = "crate::routes::serde_util::empty_string_as_none"
  )]
  status:   Option<String>,
  #[serde(
    default,
    deserialize_with = "crate::routes::serde_util::empty_string_as_none"
  )]
  system:   Option<String>,
  #[serde(
    default,
    deserialize_with = "crate::routes::serde_util::empty_string_as_none"
  )]
  job_name: Option<String>,
}

pub(in crate::routes::dashboard) async fn queue_page(
  State(state): State<AppState>,
  Query(params): Query<QueueFilterParams>,
  ctx: DashboardContext,
) -> Result<Html<String>, Response> {
  enforce_page_access(&state.config, &ctx, DashboardPage::Queue)?;
  let show_running = params.status.as_deref() != Some("pending");
  let show_pending = params.status.as_deref() != Some("running");
  let running = if show_running {
    circus_common::repo::builds::list_filtered(
      &state.pool,
      None,
      Some("running"),
      params.system.as_deref(),
      params.job_name.as_deref(),
      100,
      0,
    )
    .await
    .unwrap_or_default()
  } else {
    Vec::new()
  };
  let pending = if show_pending {
    circus_common::repo::builds::list_pending_in_scheduler_order_filtered(
      &state.pool,
      params.system.as_deref(),
      params.job_name.as_deref(),
      100,
      0,
    )
    .await
    .unwrap_or_default()
  } else {
    Vec::new()
  };

  let builders = circus_common::repo::remote_builders::list(&state.pool)
    .await
    .unwrap_or_default();
  let builder_map: HashMap<Uuid, String> =
    builders.into_iter().map(|b| (b.id, b.name)).collect();

  let agent_map = circus_common::repo::builder_sessions::list(&state.pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|s| (s.machine_id, s.name))
    .collect::<HashMap<Uuid, String>>();

  let mut context_by_eval: HashMap<Uuid, (Uuid, String, Uuid, String)> =
    HashMap::new();
  for b in running.iter().chain(pending.iter()) {
    if context_by_eval.contains_key(&b.evaluation_id) {
      continue;
    }
    let Ok(eval) =
      circus_common::repo::evaluations::get(&state.pool, b.evaluation_id).await
    else {
      continue;
    };
    let Ok(jobset) =
      circus_common::repo::jobsets::get(&state.pool, eval.jobset_id).await
    else {
      continue;
    };
    let Ok(project) =
      circus_common::repo::projects::get(&state.pool, jobset.project_id).await
    else {
      continue;
    };
    context_by_eval.insert(
      b.evaluation_id,
      (project.id, project.name, jobset.id, jobset.name),
    );
  }

  let context_for = |b: &Build| {
    context_by_eval.get(&b.evaluation_id).map_or_else(
      || (None, String::new(), None, String::new()),
      |(pid, pname, jid, jname)| {
        (Some(*pid), pname.clone(), Some(*jid), jname.clone())
      },
    )
  };

  let running_count = running.len() as i64;
  let pending_count = pending.len() as i64;

  let running_builds: Vec<QueueBuildView> = running
    .iter()
    .map(|b| {
      let elapsed = b.started_at.map_or_else(String::new, |started| {
        let dur = chrono::Utc::now() - started;
        format_elapsed(dur.num_seconds())
      });
      let builder_name = b
        .builder_id
        .and_then(|id| builder_map.get(&id).cloned())
        .or_else(|| {
          b.agent_machine_id
            .and_then(|id| agent_map.get(&id).cloned())
        });
      let (project_id, project_name, jobset_id, jobset_name) = context_for(b);
      QueueBuildView {
        id: b.id,
        job_name: b.job_name.clone(),
        project_id,
        project_name,
        jobset_id,
        jobset_name,
        system: b.system.clone().unwrap_or_else(|| "unknown".to_string()),
        created_at: b.created_at.format("%Y-%m-%d %H:%M").to_string(),
        started_at: b
          .started_at
          .map(|t| t.format("%H:%M:%S").to_string())
          .unwrap_or_default(),
        elapsed,
        started_epoch: b.started_at.map(|t| t.timestamp()),
        priority: b.priority,
        builder_name,
        queue_pos: 0,
      }
    })
    .collect();

  let pending_builds: Vec<QueueBuildView> = pending
    .iter()
    .enumerate()
    .map(|(idx, b)| {
      let (project_id, project_name, jobset_id, jobset_name) = context_for(b);
      QueueBuildView {
        id: b.id,
        job_name: b.job_name.clone(),
        project_id,
        project_name,
        jobset_id,
        jobset_name,
        system: b.system.clone().unwrap_or_else(|| "unknown".to_string()),
        created_at: b.created_at.format("%Y-%m-%d %H:%M").to_string(),
        started_at: String::new(),
        elapsed: String::new(),
        started_epoch: None,
        priority: b.priority,
        builder_name: None,
        queue_pos: (idx + 1) as i64,
      }
    })
    .collect();

  QueueTemplate {
    ui: ui_config(&state),
    pending_builds,
    running_builds,
    pending_count,
    running_count,
    show_running,
    show_pending,
    filter_status: params.status.unwrap_or_default(),
    filter_system: params.system.unwrap_or_default(),
    filter_job: params.job_name.unwrap_or_default(),
    permissions: ctx.permissions,
    csrf_token: ctx.csrf_token.clone(),
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name.clone(),
  }
  .render_html_or_500()
}
