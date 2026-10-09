//! Topcoat pages served inside axum. Forms still post to axum handlers.

use std::{future, sync::Arc, time::Duration};

use circus_common::pg_notify::{self, CHANNEL_BUILDS_CHANGED};
use tokio::sync::{Notify, watch};
use topcoat::{
  Result,
  asset::{AssetBundle, RouterBuilderAssetExt},
  context::{Cx, app_context},
  router::{
    Router,
    RouterBuilderDiscoverExt,
    page,
    query_params,
    tower::TowerService,
  },
  runtime::{RouterBuilderRuntimeExt, connected, shard},
  view::{Child, View, component, emit, live, view},
};
use uuid::Uuid;

use super::{
  components::eta_text,
  layout::{document, shard_viewer, viewer},
  pages::{Queue, QueueFilter, elapsed_since, load_queue},
  shared::{DashboardContext, DashboardPage},
};
use crate::{permissions::UiPermissions, state::AppState};

/// Bursts of build updates within this window rerender once.
const SETTLE: Duration = Duration::from_millis(500);

/// Bumped whenever a build is inserted or changes status.
#[derive(Clone)]
pub(super) struct BuildsChanged(watch::Receiver<u64>);

/// Why a live build view woke up.
pub(super) enum Wake {
  /// A build changed, so reload.
  Changed,
  /// A second passed, so redraw running clocks.
  Tick,
}

impl BuildsChanged {
  pub(super) fn subscribe(cx: &Cx) -> Self {
    let mut receiver = app_context::<Self>(cx).0.clone();
    receiver.mark_unchanged();
    Self(receiver)
  }

  /// Waits for a build change, or one second when `ticking`, then resolves the
  /// viewer again, since a run outlives the request that started it. `None`
  /// once the viewer has gone, and the redirect once they lost access.
  pub(super) async fn next(
    &mut self,
    cx: &Cx,
    page: DashboardPage,
    ticking: bool,
  ) -> Result<Option<(Wake, DashboardContext)>> {
    if !connected(cx) {
      return Ok(None);
    }

    let tick = async {
      if ticking {
        tokio::time::sleep(Duration::from_secs(1)).await;
      } else {
        future::pending::<()>().await;
      }
    };

    let wake = tokio::select! {
      changed = self.0.changed() => {
        if changed.is_err() {
          return Ok(None);
        }

        tokio::time::sleep(SETTLE).await;
        self.0.mark_unchanged();
        Wake::Changed
      },
      () = tick => Wake::Tick,
    };

    Ok(Some((wake, shard_viewer(cx, page).await?)))
  }
}

#[must_use]
pub fn service(state: AppState, assets: AssetBundle) -> TowerService {
  let wakeup = Arc::new(Notify::new());
  let (sender, receiver) = watch::channel(0);
  pg_notify::spawn_listener(
    &state.config.database.url,
    &[CHANNEL_BUILDS_CHANGED],
    Arc::clone(&wakeup),
  );
  tokio::spawn(async move {
    loop {
      wakeup.notified().await;
      sender.send_modify(|generation| *generation += 1);
    }
  });

  let router = Router::builder()
    .app_context(state)
    .app_context(BuildsChanged(receiver))
    .discover()
    .assets(assets)
    .runtime()
    .build();
  TowerService::new(router)
}

#[query_params(error = bad_request)]
struct QueueQuery {
  status:   Option<String>,
  system:   Option<String>,
  job_name: Option<String>,
}

#[page("/queue")]
async fn queue(cx: &Cx) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::Queue).await?;
  let QueueQuery {
    status,
    system,
    job_name,
  } = query_params::<QueueQuery>(cx)?;
  let selected = status.as_deref().unwrap_or_default();

  Ok(view! {
    document(title: "Queue", viewer: &viewer,
      <nav class="breadcrumbs">
        <a href="/">"Home"</a>
        <span class="sep">"/"</span>
        <span class="current">"Queue"</span>
      </nav>
      <h1>"Build Queue"</h1>
      <form method="get" action="/queue" class="filter-form">
        <label>
          "Status: "
          <select name="status">
            <option value="">"All"</option>
            <option value="pending" selected=(selected == "pending")>"Pending"</option>
            <option value="running" selected=(selected == "running")>"Running"</option>
          </select>
        </label>
        <label>
          "System: "
          <input type="text" name="system" value=(system.as_deref().unwrap_or_default()) placeholder="e.g. x86_64-linux">
        </label>
        <label>
          "Job: "
          <input type="text" name="job_name" value=(job_name.as_deref().unwrap_or_default()) placeholder="job name">
        </label>
        <button type="submit" class="btn btn-secondary">"Filter"</button>
      </form>
      queue_tables(
        status: status.clone(),
        system: system.clone(),
        job_name: job_name.clone(),
      )
    )
  })
}

/// Rerenders whenever the builds table changes, and every second while
/// builds run.
#[shard]
async fn queue_tables(
  cx: &Cx,
  status: Option<String>,
  system: Option<String>,
  job_name: Option<String>,
) -> Result<impl View> {
  let mut ctx = shard_viewer(cx, DashboardPage::Queue).await?;
  let filter = QueueFilter {
    status,
    system,
    job_name,
  };

  Ok(live! {
    let state = app_context::<AppState>(cx);
    let mut changed = BuildsChanged::subscribe(cx);
    let mut data = load_queue(state, &filter).await;

    loop {
      let ticking = data.running.as_ref().is_some_and(|running| !running.is_empty());
      let token = emit! {
        tables(data: &data, permissions: &ctx.permissions, csrf_token: &ctx.csrf_token)
      }?;

      let Some((wake, current)) = changed.next(cx, DashboardPage::Queue, ticking).await? else {
        break Ok(token);
      };
      ctx = current;

      if matches!(wake, Wake::Changed) {
        data = load_queue(state, &filter).await;
      }
    }
  })
}

#[component]
async fn tables(
  data: &Queue,
  permissions: &UiPermissions,
  csrf_token: &str,
) -> Result<impl View> {
  Ok(view! {
    if let Some(running) = &data.running {
      panel(
        kind: "running",
        title: "Running",
        count: running.len(),
        empty: "No builds currently running",
        hint: "Running builds will appear here when the queue runner picks them up.",
        <thead>
          <tr>
            <th>"Project"</th>
            <th>"Jobset"</th>
            <th>"Job"</th>
            <th>"System"</th>
            <th>"Started"</th>
            <th>"Elapsed"</th>
            <th>"Builder"</th>
          </tr>
        </thead>
        <tbody>
          for build in running {
            <tr>
              context_cells(
                project_id: build.project_id,
                project_name: &build.project_name,
                jobset_id: build.jobset_id,
                jobset_name: &build.jobset_name,
              )
              <td><a href=(format!("/build/{}", build.id))>(build.job_name.as_str())</a></td>
              <td>(build.system.as_str())</td>
              <td>(build.started_at.as_str())</td>
              <td>
                match build.started_epoch {
                  Some(started) => (elapsed_since(started)),
                  None => (build.elapsed.as_str()),
                }
                if let Some(eta) = build.eta_epoch {
                  <span class="eta">(eta_text(eta))</span>
                }
              </td>
              <td>(build.builder_name.as_deref().unwrap_or("local"))</td>
            </tr>
          }
        </tbody>
      )
    }
    if let Some(pending) = &data.pending {
      panel(
        kind: "pending",
        title: "Pending",
        count: pending.len(),
        empty: "No builds pending",
        hint: "Pending builds appear after an evaluation discovers new derivations to build.",
        <thead>
          <tr>
            <th>"#"</th>
            <th>"Project"</th>
            <th>"Jobset"</th>
            <th>"Job"</th>
            <th>"System"</th>
            <th>"Priority"</th>
            <th>"Created"</th>
            if permissions.bump_to_front {
              <th class="row-actions">"Actions"</th>
            }
          </tr>
        </thead>
        <tbody>
          for build in pending {
            <tr>
              <td>(build.queue_pos)</td>
              context_cells(
                project_id: build.project_id,
                project_name: &build.project_name,
                jobset_id: build.jobset_id,
                jobset_name: &build.jobset_name,
              )
              <td><a href=(format!("/build/{}", build.id))>(build.job_name.as_str())</a></td>
              <td>(build.system.as_str())</td>
              <td>(build.priority)</td>
              <td>(build.created_at.as_str())</td>
              if permissions.bump_to_front {
                <td class="row-actions">
                  <form method="POST" action=(format!("/build/{}/bump", build.id)) class="inline-form">
                    <input type="hidden" name="csrf_token" value=(csrf_token)>
                    <button
                      type="submit"
                      class="btn btn-small btn-secondary"
                      title="Increase this build's priority so the scheduler picks it sooner"
                    >
                      "↑ Push forward"
                    </button>
                  </form>
                </td>
              }
            </tr>
          }
        </tbody>
      )
    }
  })
}

#[component]
async fn panel(
  kind: &str,
  title: &str,
  count: usize,
  empty: &str,
  hint: &str,
  child: Child<'_>,
) -> Result<impl View> {
  Ok(view! {
    <section class=(format!("panel panel-{kind}"))>
      <div class="panel-header">
        <h2>(title)</h2>
        <span class=(format!("badge badge-{kind}"))>(count)</span>
      </div>
      if count == 0 {
        <div class="empty compact-empty">
          <div class="empty-title">(empty)</div>
          <div class="empty-hint">(hint)</div>
        </div>
      } else {
        <div class="table-wrap compact-table-wrap">
          <table>(child)</table>
        </div>
      }
    </section>
  })
}

#[component]
async fn context_cells(
  project_id: Option<Uuid>,
  project_name: &str,
  jobset_id: Option<Uuid>,
  jobset_name: &str,
) -> Result<impl View> {
  Ok(view! {
    <td>
      match project_id {
        Some(id) => <a href=(format!("/project/{id}"))>(project_name)</a>,
        None => "-",
      }
    </td>
    <td>
      match jobset_id {
        Some(id) => <a href=(format!("/jobset/{id}"))>(jobset_name)</a>,
        None => "-",
      }
    </td>
  })
}
