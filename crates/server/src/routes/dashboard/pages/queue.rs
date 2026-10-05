use std::collections::HashMap;

use circus_common::models::Build;
use uuid::Uuid;

use super::{super::shared::QueueBuildView, format_elapsed};
use crate::state::AppState;

#[derive(Clone, Default)]
pub(in crate::routes::dashboard) struct QueueFilter {
  pub(in crate::routes::dashboard) status:   Option<String>,
  pub(in crate::routes::dashboard) system:   Option<String>,
  pub(in crate::routes::dashboard) job_name: Option<String>,
}

pub(in crate::routes::dashboard) struct Queue {
  pub(in crate::routes::dashboard) running: Option<Vec<QueueBuildView>>,
  pub(in crate::routes::dashboard) pending: Option<Vec<QueueBuildView>>,
}

pub(in crate::routes::dashboard) async fn load(
  state: &AppState,
  filter: &QueueFilter,
) -> Queue {
  let show_running = filter.status.as_deref() != Some("pending");
  let show_pending = filter.status.as_deref() != Some("running");
  let running = if show_running {
    circus_common::repo::builds::list_filtered(
      &state.pool,
      None,
      Some("running"),
      filter.system.as_deref(),
      filter.job_name.as_deref(),
      Some("build"),
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
      filter.system.as_deref(),
      filter.job_name.as_deref(),
      100,
      0,
    )
    .await
    .unwrap_or_default()
  } else {
    Vec::new()
  };

  let agent_map = circus_common::repo::builder_sessions::list(&state.pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|s| (s.machine_id, s.name))
    .collect::<HashMap<Uuid, String>>();

  let running_ids = running.iter().map(|build| build.id).collect::<Vec<Uuid>>();
  let expected =
    circus_common::repo::builds::expected_durations(&state.pool, &running_ids)
      .await
      .unwrap_or_else(|error| {
        tracing::warn!("Failed to estimate running build durations: {error}");
        HashMap::new()
      });

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

  let running_builds: Vec<QueueBuildView> = running
    .iter()
    .map(|b| {
      let elapsed = b.started_at.map_or_else(String::new, |started| {
        format_elapsed(jiff::Timestamp::now().duration_since(started).as_secs())
      });
      let builder_name = b
        .agent_machine_id
        .and_then(|id| agent_map.get(&id).cloned());
      let (project_id, project_name, jobset_id, jobset_name) = context_for(b);
      QueueBuildView {
        id: b.id,
        job_name: b.job_name.clone(),
        project_id,
        project_name,
        jobset_id,
        jobset_name,
        system: b.system.clone().unwrap_or_else(|| "unknown".to_string()),
        created_at: b.created_at.strftime("%Y-%m-%d %H:%M").to_string(),
        started_at: b
          .started_at
          .map(|t| t.strftime("%H:%M:%S").to_string())
          .unwrap_or_default(),
        elapsed,
        started_epoch: b.started_at.map(jiff::Timestamp::as_second),
        eta_epoch: b
          .started_at
          .zip(expected.get(&b.id))
          .map(|(started, secs)| started.as_second() + secs),
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
        created_at: b.created_at.strftime("%Y-%m-%d %H:%M").to_string(),
        started_at: String::new(),
        elapsed: String::new(),
        started_epoch: None,
        eta_epoch: None,
        priority: b.priority,
        builder_name: None,
        queue_pos: (idx + 1) as i64,
      }
    })
    .collect();

  Queue {
    running: show_running.then_some(running_builds),
    pending: show_pending.then_some(pending_builds),
  }
}
