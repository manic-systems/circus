//! Topcoat pages served inside axum. Forms still post to axum handlers.

use std::{sync::Arc, time::Duration};

use askama::Template;
use circus_common::pg_notify::{self, CHANNEL_BUILDS_CHANGED};
use circus_config::PageAccessLevel;
use tokio::sync::{Notify, watch};
use topcoat::{
  Result,
  asset::{AssetBundle, RouterBuilderAssetExt},
  context::{Cx, app_context},
  router::{
    Router,
    error::{internal_server_error, see_other},
    page,
    query_params,
    request,
    tower::TowerService,
  },
  runtime::{RouterBuilderRuntimeExt, connected, shard},
  view::{Child, Unescaped, View, component, emit, live, view},
};
use uuid::Uuid;

use super::{
  pages::{Queue, QueueFilter, load_queue},
  shared::{DashboardContext, DashboardPage, UiTemplateConfig},
  templates::{LIVE_SLOT, LiveShellTemplate},
};
use crate::{
  auth_middleware::session_extensions,
  permissions::UiPermissions,
  state::AppState,
};

/// Bursts of build updates within this window rerender once.
const SETTLE: Duration = Duration::from_millis(500);

/// Bumped whenever a build is inserted or changes status.
#[derive(Clone)]
struct BuildsChanged(watch::Receiver<u64>);

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
    .page(queue)
    .route(queue_tables)
    .assets(assets)
    .runtime()
    .build();
  TowerService::new(router)
}

/// WebSocket reruns skip axum's middleware, so read the viewer from the cookie.
async fn viewer(cx: &Cx, page: DashboardPage) -> Result<DashboardContext> {
  let state = app_context::<AppState>(cx);
  let session = session_extensions(state, request::headers(cx)).await;
  let ctx = DashboardContext::from_extensions(&session);
  let allowed = match page.access(&state.config.server) {
    PageAccessLevel::Public => true,
    PageAccessLevel::Authenticated => ctx.is_authenticated,
    PageAccessLevel::Admin => ctx.is_admin,
  };

  if allowed {
    Ok(ctx)
  } else if ctx.is_authenticated {
    Err(see_other("/").into())
  } else {
    Err(see_other("/login").into())
  }
}

#[component]
async fn document(
  cx: &Cx,
  title: &str,
  viewer: &DashboardContext,
  child: Child<'_>,
) -> Result<impl View> {
  let state = app_context::<AppState>(cx);
  let shell = LiveShellTemplate {
    ui: UiTemplateConfig::from_config(&state.config.ui),
    title,
    is_admin: viewer.is_admin,
    auth_name: &viewer.auth_name,
  }
  .render()
  .map_err(internal_server_error)?;
  let (before, after) = shell
    .split_once(LIVE_SLOT)
    .map(|(before, after)| (before.to_owned(), after.to_owned()))
    .ok_or_else(|| {
      internal_server_error(std::io::Error::other("live shell lost its slot"))
    })?;

  Ok(view! {
    (Unescaped::new_unchecked(before))
    (child)
    topcoat::runtime::script()
    (Unescaped::new_unchecked(after))
  })
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

/// Rerenders whenever the builds table changes.
#[shard]
async fn queue_tables(
  cx: &Cx,
  status: Option<String>,
  system: Option<String>,
  job_name: Option<String>,
) -> Result<impl View> {
  let mut ctx = viewer(cx, DashboardPage::Queue).await?;
  let filter = QueueFilter {
    status,
    system,
    job_name,
  };

  Ok(live! {
    let state = app_context::<AppState>(cx);
    let mut changed = app_context::<BuildsChanged>(cx).0.clone();
    changed.mark_unchanged();

    loop {
      let data = load_queue(state, &filter).await;
      let token = emit! {
        tables(data: data, permissions: &ctx.permissions, csrf_token: &ctx.csrf_token)
      }?;

      if !connected(cx) || changed.changed().await.is_err() {
        break Ok(token);
      }

      tokio::time::sleep(SETTLE).await;
      changed.mark_unchanged();

      // The run outlives the request that started it, so the session may
      // have ended or lost access since. Its redirect still reaches the page.
      match viewer(cx, DashboardPage::Queue).await {
        Ok(current) => ctx = current,
        Err(error) => break Err(error),
      }
    }
  })
}

#[component]
async fn tables(
  data: Queue,
  permissions: &UiPermissions,
  csrf_token: &str,
) -> Result<impl View> {
  Ok(view! {
    if let Some(running) = data.running {
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
                project_name: build.project_name,
                jobset_id: build.jobset_id,
                jobset_name: build.jobset_name,
              )
              <td><a href=(format!("/build/{}", build.id))>(build.job_name)</a></td>
              <td>(build.system)</td>
              <td>(build.started_at)</td>
              <td data-live-elapsed=(build.started_epoch)>(build.elapsed)</td>
              <td>(build.builder_name.as_deref().unwrap_or("local"))</td>
            </tr>
          }
        </tbody>
      )
    }
    if let Some(pending) = data.pending {
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
                project_name: build.project_name,
                jobset_id: build.jobset_id,
                jobset_name: build.jobset_name,
              )
              <td><a href=(format!("/build/{}", build.id))>(build.job_name)</a></td>
              <td>(build.system)</td>
              <td>(build.priority)</td>
              <td>(build.created_at)</td>
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
  project_name: String,
  jobset_id: Option<Uuid>,
  jobset_name: String,
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
