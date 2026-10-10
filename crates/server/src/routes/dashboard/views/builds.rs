//! The dashboard home, the build list, a build, and its log.

use std::{collections::HashMap, path::Path, time::Duration};

use circus_common::models::{BuildProduct, NewsItem};
use jiff::Timestamp;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{error::RouterErrorExt, page, path_param, query_params},
  runtime::{connected, shard},
  view::{View, component, emit, live, view},
};
use uuid::Uuid;

use super::super::{
  build_log::{BuildLogView, parse_build_log},
  layout::{document, viewer},
  pages::{
    dashboard_system_filters,
    derivation_name,
    format_elapsed,
    is_failed_status,
  },
  shared::{
    BrokeInView,
    BuildView,
    DashboardPage,
    EvalView,
    Pagination,
    ProjectSummaryView,
    QueueSystemView,
    WorkerSummaryView,
    build_view,
    build_view_with_context,
    eval_view,
  },
};
use crate::{operator, state::AppState};

path_param!(id: Uuid, error = not_found);

#[query_params(error = bad_request)]
struct HomeQuery {
  q:      Option<String>,
  status: Option<String>,
}

#[page("/")]
async fn home_page(cx: &Cx) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::Home).await?;
  let state = app_context::<AppState>(cx);
  let query = query_params::<HomeQuery>(cx)?;
  let search = query.q.clone().unwrap_or_default();
  let status = query.status.clone().unwrap_or_default();
  let include_hidden = viewer.is_admin;
  let overview = operator::overview(state, include_hidden)
    .await
    .map_err(|error| error.0)?;
  let recent_evals: Vec<EvalView> =
    circus_common::repo::evaluations::list_filtered_with_visibility(
      &state.pool,
      None,
      None,
      5,
      0,
      include_hidden,
    )
    .await
    .unwrap_or_default()
    .iter()
    .map(eval_view)
    .collect();
  let announcements = circus_common::repo::news::list(&state.pool, 3, 0)
    .await
    .unwrap_or_default();
  let needle = search.trim().to_lowercase();
  let recent_builds: Vec<BuildView> = overview
    .recent_builds
    .iter()
    .map(BuildView::from)
    .filter(|build| status.is_empty() || build.status_class == status)
    .filter(|build| {
      needle.is_empty()
        || [&build.project_name, &build.job_name, &build.system]
          .iter()
          .any(|field| field.to_lowercase().contains(&needle))
    })
    .collect();
  let totals = Totals {
    total:     overview.total_builds,
    succeeded: overview.completed_builds,
    failed:    overview.failed_builds,
    running:   overview.running_builds,
    pending:   overview.pending_builds,
  };

  Ok(view! {
    document(title: "Dashboard", viewer: &viewer, topbar: false,
      <header class="page-header operator-header">
        <div>
          <div class="dashboard-title-row">
            <h1>"Dashboard"</h1>
          </div>
          <p class="page-subtitle">"Evaluations, builds, agents, and project health."</p>
        </div>
        <div class="operator-header-actions">
          if viewer.auth_name.is_empty() {
            <a class="btn btn-secondary" href="/login">"Login"</a>
          } else {
            <a class="auth-user" href="/account">(viewer.auth_name.clone())</a>
            <form method="POST" action="/logout">
              <button class="btn-ghost" type="submit">"Logout"</button>
            </form>
          }
        </div>
        <form class="operator-toolbar" method="get" action="/builds">
          <input
            class="dashboard-search"
            type="search"
            name="job_name"
            placeholder="Filter builds by job name"
            aria-label="Filter builds by job name"
          >
          <select class="dashboard-system" name="system" aria-label="Filter builds by system">
            <option value="">"All systems"</option>
            for system in dashboard_system_filters(&overview) {
              <option value=(system.clone())>(system)</option>
            }
          </select>
          <button class="btn btn-small btn-secondary" type="submit">"View builds"</button>
        </form>
      </header>
      announcement_lines(items: announcements)
      metric_strip(totals: totals)
      <section class="dashboard-grid" aria-label="Build farm overview">
        <div class="operator-primary">
          projects_panel(
            projects: overview.projects.iter().map(ProjectSummaryView::from).collect(),
            is_admin: viewer.is_admin,
          )
          recent_builds_panel(builds: recent_builds, search: search, status: status)
          agents_panel(
            workers: overview.workers.iter().map(WorkerSummaryView::from).collect(),
            show_names: !viewer.auth_name.is_empty(),
          )
        </div>
        <aside class="operator-sidebar" aria-label="Build farm inspector">
          queue_panel(
            queue: overview.queue_by_system.iter().map(QueueSystemView::from).collect(),
          )
          recent_evals_panel(evals: recent_evals)
        </aside>
      </section>
    )
  })
}

#[component]
async fn announcement_lines(items: Vec<NewsItem>) -> Result<impl View> {
  Ok(view! {
    if !items.is_empty() {
      <div class="announcements" role="status">
        for item in items {
          <div class="announcement-line">
            <span><strong>(item.title)</strong> ": " (item.content)</span>
            <button
              type="button"
              class="btn-ghost announcement-close"
              aria-label="Dismiss announcement"
              onclick="this.parentElement.remove()"
            >
              "×"
            </button>
          </div>
        }
      </div>
    }
  })
}

struct Totals {
  total:     i64,
  succeeded: i64,
  failed:    i64,
  running:   i64,
  pending:   i64,
}

const fn share(part: i64, whole: i64) -> i64 {
  if whole > 0 { part * 100 / whole } else { 0 }
}

#[component]
async fn metric_strip(totals: Totals) -> Result<impl View> {
  let finished = totals.total - totals.running - totals.pending;
  let success_rate = if finished > 0 {
    format!("{}%", totals.succeeded * 100 / finished)
  } else {
    "-".to_owned()
  };

  Ok(view! {
    <section class="metric-strip" aria-label="Build farm metrics">
      <a class="metric-cell" href="/builds">
        <span>"Total builds"</span>
        <strong>(totals.total)</strong>
        <small>"all tracked builds"</small>
      </a>
      <a class="metric-cell metric-success" href="/builds?status=succeeded">
        <span>"Succeeded"</span>
        <strong>(totals.succeeded)</strong>
        <small>(share(totals.succeeded, totals.total)) "% of all builds"</small>
      </a>
      <a class="metric-cell metric-failed" href="/builds?status=failed">
        <span>"Failed"</span>
        <strong>(totals.failed)</strong>
        <small>(share(totals.failed, totals.total)) "% of all builds"</small>
      </a>
      <a class="metric-cell metric-running" href="/builds?status=running">
        <span>"Running"</span>
        <strong>(totals.running)</strong>
        <small>"currently running"</small>
      </a>
      <a class="metric-cell" href="/queue">
        <span>"Queued"</span>
        <strong>(totals.pending)</strong>
        <small>"pending now"</small>
      </a>
      <a class="metric-cell metric-success" href="/builds?status=succeeded">
        <span>"Success rate"</span>
        <strong>(success_rate)</strong>
        <small>"completed / finished"</small>
      </a>
    </section>
  })
}

#[component]
async fn projects_panel(
  projects: Vec<ProjectSummaryView>,
  is_admin: bool,
) -> Result<impl View> {
  Ok(view! {
    <section class="panel projects-panel">
      <div class="panel-header">
        <h2>"Projects"</h2>
        <span class="panel-actions">
          <a href="/projects">"all projects"</a>
          if is_admin {
            <a href="/projects/new">"new project"</a>
          }
        </span>
      </div>
      if projects.is_empty() {
        <div class="empty compact-empty">
          <div class="empty-title">"No projects yet"</div>
          if is_admin {
            <div class="empty-hint">
              <a href="/projects/new">"Create a project"</a>
              " to get started."
            </div>
          }
        </div>
      } else {
        <div class="table-wrap compact-table-wrap">
          <table class="data-table dense-table">
            <thead>
              <tr>
                <th>"Project"</th>
                <th class="numeric" style="width:5rem">"Jobsets"</th>
                <th>"Last eval"</th>
                <th>"Eval status"</th>
                <th class="numeric" style="width:6rem">"Failing jobs"</th>
                <th class="numeric" style="width:5rem">"Queued"</th>
                <th>"Systems"</th>
                <th>"Updated"</th>
              </tr>
            </thead>
            <tbody>
              for project in projects {
                <tr>
                  <td class="truncate" title=(project.name.clone())>
                    <a href=(format!("/project/{}", project.id))>(project.name)</a>
                  </td>
                  <td class="numeric">(project.jobset_count)</td>
                  <td class="mono nowrap">(project.last_eval_time)</td>
                  <td>
                    <span class=(format!("status status-{}", project.last_eval_class))>
                      (project.last_eval_status)
                    </span>
                  </td>
                  <td class="numeric">
                    <a href="/builds?status=failed">(project.failing_jobs)</a>
                  </td>
                  <td class="numeric">
                    <a href="/queue">(project.queued_jobs)</a>
                  </td>
                  <td class="mono truncate" title=(project.systems.clone())>(project.systems)</td>
                  <td class="mono nowrap">(project.updated_at)</td>
                </tr>
              }
            </tbody>
          </table>
        </div>
      }
    </section>
  })
}

#[component]
async fn recent_builds_panel(
  builds: Vec<BuildView>,
  search: String,
  status: String,
) -> Result<impl View> {
  let options = [
    ("", "All status"),
    ("failed", "Failed"),
    ("succeeded", "Succeeded"),
    ("running", "Running"),
    ("pending", "Queued"),
  ];

  Ok(view! {
    <section class="panel panel-recent">
      <div class="panel-header">
        <h2>"Builds"</h2>
        <a href="/builds">"all builds"</a>
      </div>
      <form class="filter-bar" method="get" action="/">
        <input
          type="search"
          name="q"
          value=(search)
          aria-label="Filter recent builds"
          placeholder="filter project, job, system"
        >
        <select name="status" aria-label="Filter recent builds by status">
          for (value, label) in options {
            <option value=(value) selected=(status == value)>(label)</option>
          }
        </select>
        <button class="btn btn-small btn-secondary" type="submit">"Filter"</button>
      </form>
      if builds.is_empty() {
        <div class="empty compact-empty">
          <div class="empty-title">"No builds yet"</div>
          <div class="empty-hint">"Builds appear once an evaluation produces jobs."</div>
        </div>
      } else {
        <div class="table-wrap compact-table-wrap">
          <table class="data-table dense-table">
            <thead>
              <tr>
                <th>"Build"</th>
                <th>"Project"</th>
                <th>"Job"</th>
                <th>"System"</th>
                <th>"Status"</th>
                <th>"Duration"</th>
                <th>"Created"</th>
                <th>"Log"</th>
              </tr>
            </thead>
            <tbody>
              for build in builds {
                <tr>
                  build_id_cell(id: build.id, id_short: build.id_short)
                  <td class="truncate" title=(build.project_name.clone())>
                    entity_link(kind: "project", id: build.project_id, name: build.project_name)
                  </td>
                  <td class="mono truncate" title=(build.job_name.clone())>(build.job_name)</td>
                  <td class="mono nowrap">(build.system)</td>
                  <td>
                    <span class=(format!("status status-{}", build.status_class))>(build.status_text)</span>
                  </td>
                  <td class="mono">(build.duration)</td>
                  <td class="mono nowrap">(build.created_at)</td>
                  log_cell(id: build.id, has_log: build.has_log)
                </tr>
              }
            </tbody>
          </table>
        </div>
      }
    </section>
  })
}

#[component]
async fn agents_panel(
  workers: Vec<WorkerSummaryView>,
  show_names: bool,
) -> Result<impl View> {
  Ok(view! {
    <section class="panel panel-agents">
      <div class="panel-header">
        <h2>"Agents"</h2>
        <a href="/admin#agents">"manage"</a>
      </div>
      if workers.is_empty() {
        <div class="empty compact-empty">
          <div class="empty-title">"No agents"</div>
          <div class="empty-hint">"No agent sessions have checked in."</div>
        </div>
      } else {
        <div class="table-wrap compact-table-wrap">
          <table class="data-table dense-table">
            <thead>
              <tr>
                <th>"Agent"</th>
                <th>"System"</th>
                <th>"Status"</th>
                <th>"Load"</th>
              </tr>
            </thead>
            <tbody>
              for (index, worker) in workers.into_iter().enumerate() {
                <tr>
                  <td class="truncate">
                    if show_names {
                      <strong title=(worker.name.clone())>(worker.name)</strong>
                    } else {
                      <strong>"Agent #" (index + 1)</strong>
                    }
                  </td>
                  <td class="mono nowrap">(worker.system)</td>
                  <td>
                    <span class=(format!("status status-{}", worker.status_class))>(worker.status_text)</span>
                  </td>
                  <td class="mono nowrap">(worker.current_jobs) "/" (worker.max_jobs)</td>
                </tr>
              }
            </tbody>
          </table>
        </div>
      }
    </section>
  })
}

#[component]
async fn queue_panel(queue: Vec<QueueSystemView>) -> Result<impl View> {
  Ok(view! {
    <section class="panel panel-queue">
      <div class="panel-header">
        <h2>"Queue"</h2>
        <a href="/queue">"queue"</a>
      </div>
      <h3>"Queued by system"</h3>
      if queue.is_empty() {
        <div class="section-empty">"No queued builds."</div>
      } else {
        <ul class="summary-list">
          for item in queue {
            <li>
              <a href=(format!("/builds?status=pending&system={}", item.system))>
                <span class="mono">(item.system)</span>
                <strong class="numeric">(item.count)</strong>
              </a>
            </li>
          }
        </ul>
      }
    </section>
  })
}

#[component]
async fn recent_evals_panel(evals: Vec<EvalView>) -> Result<impl View> {
  Ok(view! {
    <section class="panel panel-recent-evals">
      <div class="panel-header">
        <h2>"Recent evaluations"</h2>
        <a href="/evaluations">"all"</a>
      </div>
      if evals.is_empty() {
        <div class="section-empty">"No recent evaluations."</div>
      } else {
        <ul class="summary-list">
          for eval in evals {
            <li>
              <a href=(format!("/evaluation/{}", eval.id))>
                <span class="mono">(eval.commit_short)</span>
                <span class=(format!("status status-{}", eval.status_class))>(eval.status_text)</span>
              </a>
            </li>
          }
        </ul>
      }
    </section>
  })
}

#[component]
async fn build_id_cell(id: Uuid, id_short: String) -> Result<impl View> {
  Ok(view! {
    <td class="mono">
      <a href=(format!("/build/{id}")) title=(id.to_string()) aria-label=(format!("Build {id}"))>
        "#" (id_short)
      </a>
    </td>
  })
}

/// A link to a project or jobset, or a dash when the build has lost it.
#[component]
async fn entity_link(
  kind: &str,
  id: Option<Uuid>,
  name: String,
) -> Result<impl View> {
  Ok(view! {
    match id {
      Some(id) => <a href=(format!("/{kind}/{id}"))>(name)</a>,
      None => "-",
    }
  })
}

#[component]
async fn log_cell(id: Uuid, has_log: bool) -> Result<impl View> {
  Ok(view! {
    <td>
      if has_log {
        <a href=(format!("/build/{id}/log"))>"log"</a>
      } else {
        "-"
      }
    </td>
  })
}

#[query_params(error = bad_request)]
pub(in crate::routes::dashboard) struct BuildFilterParams {
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
  limit:    Option<i64>,
  offset:   Option<i64>,
}

#[page("/builds")]
async fn builds_page(cx: &Cx) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::Builds).await?;
  let state = app_context::<AppState>(cx);
  let params = query_params::<BuildFilterParams>(cx)?;
  let limit = params.limit.unwrap_or(50).clamp(1, 200);
  let offset = params.offset.unwrap_or(0).max(0);
  let items = circus_common::repo::builds::list_filtered(
    &state.pool,
    None,
    params.status.as_deref(),
    params.system.as_deref(),
    params.job_name.as_deref(),
    limit,
    offset,
  )
  .await
  .unwrap_or_default();
  let total = circus_common::repo::builds::count_filtered(
    &state.pool,
    None,
    params.status.as_deref(),
    params.system.as_deref(),
    params.job_name.as_deref(),
  )
  .await
  .unwrap_or(0);
  let pagination = Pagination::new(total, offset, limit);

  let mut context_by_eval = HashMap::new();
  for item in &items {
    if context_by_eval.contains_key(&item.evaluation_id) {
      continue;
    }

    let Ok(eval) =
      circus_common::repo::evaluations::get(&state.pool, item.evaluation_id)
        .await
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
      item.evaluation_id,
      (project.id, project.name, jobset.id, jobset.name),
    );
  }

  let rows: Vec<BuildView> = items
    .iter()
    .map(|item| {
      context_by_eval.get(&item.evaluation_id).map_or_else(
        || build_view(item),
        |(project_id, project_name, jobset_id, jobset_name)| {
          build_view_with_context(
            item,
            *project_id,
            project_name,
            *jobset_id,
            jobset_name,
          )
        },
      )
    })
    .collect();
  let status = params.status.clone().unwrap_or_default();
  let system = params.system.clone().unwrap_or_default();
  let job = params.job_name.clone().unwrap_or_default();
  let page_href = |page_offset: i64| {
    let query: String = url::form_urlencoded::Serializer::new(String::new())
      .append_pair("offset", &page_offset.to_string())
      .append_pair("limit", &limit.to_string())
      .append_pair("status", &status)
      .append_pair("system", &system)
      .append_pair("job_name", &job)
      .finish();
    format!("/builds?{query}")
  };
  let prev_href = page_href(pagination.prev_offset);
  let next_href = page_href(pagination.next_offset);
  let statuses = [
    ("", "All"),
    ("pending", "Pending"),
    ("running", "Running"),
    ("succeeded", "Succeeded"),
    ("failed", "Failed"),
  ];

  Ok(view! {
    document(title: "Builds", viewer: &viewer,
      <h1>"Builds"</h1>
      <form method="get" action="/builds" class="filter-form">
        <label>
          "Status: "
          <select name="status">
            for (value, label) in statuses {
              <option value=(value) selected=(status == value)>(label)</option>
            }
          </select>
        </label>
        <label>
          "System: "
          <input type="text" name="system" value=(system.clone()) placeholder="e.g. x86_64-linux">
        </label>
        <label>
          "Job: "
          <input type="text" name="job_name" value=(job.clone()) placeholder="job name">
        </label>
        <button type="submit" class="btn btn-secondary">"Filter"</button>
      </form>
      if rows.is_empty() {
        <div class="empty">
          <div class="empty-title">"No builds match filters"</div>
          <div class="empty-hint">"Try adjusting the filters above or wait for builds to be queued."</div>
        </div>
      } else {
        <div class="table-wrap">
          <table class="data-table dense-table">
            <thead>
              <tr>
                <th>"Build"</th>
                <th>"Project"</th>
                <th>"Jobset"</th>
                <th>"Job"</th>
                <th>"System"</th>
                <th>"Status"</th>
                <th>"Duration"</th>
                <th>"Created"</th>
                <th>"Log"</th>
              </tr>
            </thead>
            <tbody>
              for build in rows {
                <tr>
                  build_id_cell(id: build.id, id_short: build.id_short)
                  <td class="truncate" title=(build.project_name.clone())>
                    entity_link(kind: "project", id: build.project_id, name: build.project_name)
                  </td>
                  <td class="mono truncate">
                    entity_link(kind: "jobset", id: build.jobset_id, name: build.jobset_name)
                  </td>
                  <td class="mono truncate" title=(build.job_name.clone())>
                    <a href=(format!("/build/{}", build.id))>(build.job_name)</a>
                  </td>
                  <td class="mono nowrap">(build.system)</td>
                  <td>
                    <span class=(format!("status status-{}", build.status_class))>(build.status_text)</span>
                  </td>
                  <td class="mono">(build.duration)</td>
                  <td class="mono nowrap">(build.created_at)</td>
                  log_cell(id: build.id, has_log: build.has_log)
                </tr>
              }
            </tbody>
          </table>
        </div>
        if pagination.total_pages > 1 {
          <nav class="pagination">
            if pagination.has_prev {
              <a href=(prev_href) class="btn btn-small btn-secondary">"« Previous"</a>
            }
            <span class="text-muted">"Page " (pagination.page) " of " (pagination.total_pages)</span>
            if pagination.has_next {
              <a href=(next_href) class="btn btn-small btn-secondary">"Next »"</a>
            }
          </nav>
        }
      }
    )
  })
}

struct Lineage {
  eval_id:      Uuid,
  commit_short: String,
  jobset_id:    Uuid,
  jobset_name:  String,
  project_id:   Uuid,
  project_name: String,
}

async fn lineage(state: &AppState, evaluation_id: Uuid) -> Result<Lineage> {
  let eval = circus_common::repo::evaluations::get(&state.pool, evaluation_id)
    .await
    .ok()
    .ok_or_not_found()?;
  let jobset = circus_common::repo::jobsets::get(&state.pool, eval.jobset_id)
    .await
    .ok()
    .ok_or_not_found()?;
  let project =
    circus_common::repo::projects::get(&state.pool, jobset.project_id)
      .await
      .ok()
      .ok_or_not_found()?;

  Ok(Lineage {
    eval_id:      eval.id,
    commit_short: eval.commit_hash.chars().take(12).collect(),
    jobset_id:    jobset.id,
    jobset_name:  jobset.name,
    project_id:   project.id,
    project_name: project.name,
  })
}

#[page("/build/{id}")]
async fn build_page(cx: &Cx) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::Build).await?;
  let state = app_context::<AppState>(cx);
  let id = *path_param::<Id>(cx)?;
  let build = circus_common::repo::builds::get(&state.pool, id)
    .await
    .ok()
    .ok_or_not_found()?;
  let lineage = lineage(state, build.evaluation_id).await?;
  let products =
    circus_common::repo::build_products::list_for_build(&state.pool, id)
      .await
      .unwrap_or_default();
  let dependencies: Vec<BuildView> =
    circus_common::repo::build_dependencies::list_dependency_builds(
      &state.pool,
      id,
    )
    .await
    .unwrap_or_default()
    .iter()
    .map(build_view)
    .collect();
  let dependents: Vec<BuildView> =
    circus_common::repo::build_dependencies::list_dependent_builds(
      &state.pool,
      id,
    )
    .await
    .unwrap_or_default()
    .iter()
    .map(build_view)
    .collect();
  let broke_in = if is_failed_status(build.status) {
    circus_common::repo::builds::broke_in(&state.pool, id)
      .await
      .unwrap_or_else(|error| {
        tracing::warn!(build_id = %id, "Failed to find where the job broke: {error}");
        None
      })
      .map(|found| {
        BrokeInView {
          build_id:              found.build_id,
          commit_short:          found.commit_hash.chars().take(12).collect(),
          commit_subject:        found.commit_subject.unwrap_or_default(),
          last_success_build_id: found.last_success_build_id,
          last_success_short:    found
            .last_success_commit
            .chars()
            .take(12)
            .collect(),
        }
      })
  } else {
    None
  };
  let builder_label = match build.agent_machine_id {
    Some(machine_id) => {
      circus_common::repo::builder_sessions::get(&state.pool, machine_id)
        .await
        .map_or_else(|_| "local".to_owned(), |session| session.name)
    },
    None => "local".to_owned(),
  };
  let build = build_view(&build);
  let title = format!("Build {}", build.job_name);

  Ok(view! {
    document(title: &title, viewer: &viewer,
      <nav class="breadcrumbs">
        <a href="/projects">"Projects"</a>
        <span class="sep">"/"</span>
        <a href=(format!("/project/{}", lineage.project_id))>(lineage.project_name.clone())</a>
        <span class="sep">"/"</span>
        <a href=(format!("/project/{}#jobsets", lineage.project_id))>"Jobsets"</a>
        <span class="sep">"/"</span>
        <a href=(format!("/jobset/{}", lineage.jobset_id))>(lineage.jobset_name.clone())</a>
        <span class="sep">"/"</span>
        <a href=(format!("/evaluation/{}", lineage.eval_id))>(lineage.commit_short.clone())</a>
      </nav>
      <h1>"Build: " (build.job_name.clone())</h1>
      build_overview(details: &build, builder_label: builder_label, broke_in: broke_in)
      <section class="panel build-detail-section">
        <div class="panel-header">
          <h2>"Reproduce This Build"</h2>
        </div>
        <div class="panel-body">
          <p class="text-muted">"To reproduce this build locally, run one of the following commands:"</p>
          <div class="code-block">
            <strong>"Using Nix (flakes):"</strong>
            <pre><code>"nix build " (build.drv_path.clone()) "^*"</code></pre>
          </div>
          <div class="code-block">
            <strong>"Using legacy nix-build:"</strong>
            <pre><code>"nix-build " (build.drv_path.clone())</code></pre>
          </div>
          <p class="text-muted">
            "Note: You may need to add this server as a substituter to avoid rebuilding dependencies."
          </p>
        </div>
      </section>
      <section class="panel build-detail-section">
        <div class="panel-header">
          <h2>"Dependencies"</h2>
        </div>
        if dependencies.is_empty() && dependents.is_empty() {
          <div class="empty compact-empty">
            <div class="empty-title">"No build dependency relationships recorded."</div>
          </div>
        } else {
          <div class="panel-body">
            <div class="split-grid">
              related_builds(title: "Required Builds", empty: "No dependencies.", builds: dependencies)
              related_builds(title: "Dependent Builds", empty: "No dependents.", builds: dependents)
            </div>
          </div>
        }
      </section>
      products_panel(build_id: build.id, products: products)
    )
  })
}

#[component]
async fn build_overview(
  details: &BuildView,
  builder_label: String,
  broke_in: Option<BrokeInView>,
) -> Result<impl View> {
  let signed = if details.signed { "Yes" } else { "No" };

  Ok(view! {
    <section class="panel build-detail-section">
      <div class="panel-header">
        <h2>"Overview"</h2>
      </div>
      <div class="panel-body">
        <div class="build-meta">
          meta_item(label: "Status",
            <span class=(format!("badge badge-{}", details.status_class))>(details.status_text.clone())</span>
          )
          meta_item(label: "System", (details.system.clone()))
          meta_item(label: "Builder", (builder_label))
          meta_item(label: "Created", (details.created_at.clone()))
          if !details.started_at.is_empty() {
            meta_item(label: "Started", (details.started_at.clone()))
          }
          if !details.completed_at.is_empty() {
            meta_item(label: "Completed", (details.completed_at.clone()))
          }
          match (details.started_epoch, details.duration.is_empty()) {
            (Some(started), true) => meta_item(label: "Elapsed", elapsed(started: started)),
            (Some(started), false) => meta_item(label: "Duration", elapsed(started: started)),
            (None, false) => meta_item(label: "Duration", (details.duration.clone())),
            (None, true) => {},
          }
          meta_item(label: "Priority", (details.priority))
          meta_item(label: "Signed", (signed))
          if details.is_aggregate {
            meta_item(label: "Aggregate", "Yes")
          }
          meta_item(label: "Derivation", <code>(details.drv_path.clone())</code>)
          if !details.output_path.is_empty() {
            meta_item(label: "Output", <code>(details.output_path.clone())</code>)
          }
          if let Some(broke) = broke_in {
            <div class="build-meta-item build-meta-wide">
              <span class="build-meta-label">"Broke in"</span>
              <span class="build-meta-value">
                if broke.build_id == details.id {
                  "This evaluation"
                } else {
                  <a href=(format!("/build/{}", broke.build_id))><code class="commit-ref">(broke.commit_short)</code></a>
                }
                if !broke.commit_subject.is_empty() {
                  <span class="commit-message">(broke.commit_subject)</span>
                }
                <span class="broke-in-since">
                  "Last passed at "
                  <a href=(format!("/build/{}", broke.last_success_build_id))><code class="commit-ref">(broke.last_success_short)</code></a>
                </span>
              </span>
            </div>
          }
        </div>
        if !details.error_lines.is_empty() {
          <section class="build-error">
            <h2 class="section-title">"Errors"</h2>
            <ul class="error-log">
              for line in &details.error_lines {
                <li class=(format!("error-log-line error-log-{}", line.level))>(line.text.clone())</li>
              }
            </ul>
          </section>
        } else if !details.error_message.is_empty() {
          <section class="build-error">
            <h2 class="section-title">"Errors"</h2>
            <pre class="error-log-raw">(details.error_message.clone())</pre>
          </section>
        }
        if details.has_log {
          <p>
            <a class="btn btn-small btn-secondary" href=(format!("/build/{}/log", details.id))>"View full log"</a>
          </p>
        }
      </div>
    </section>
  })
}

#[component]
async fn meta_item(
  label: &str,
  child: topcoat::view::Child<'_>,
) -> Result<impl View> {
  Ok(view! {
    <div class="build-meta-item">
      <span class="build-meta-label">(label)</span>
      <span class="build-meta-value">(child)</span>
    </div>
  })
}

/// A running build's elapsed time, ticking once a second while the page is
/// open.
#[shard]
async fn elapsed(cx: &Cx, started: i64) -> Result<impl View> {
  Ok(live! {
    let mut tick = tokio::time::interval(Duration::from_secs(1));

    loop {
      tick.tick().await;
      let secs = (Timestamp::now().as_second() - started).max(0);
      let token = emit! { (format_elapsed(secs)) }?;

      if !connected(cx) {
        break Ok(token);
      }
    }
  })
}

#[component]
async fn related_builds(
  title: &str,
  empty: &str,
  builds: Vec<BuildView>,
) -> Result<impl View> {
  Ok(view! {
    <section>
      <h3 class="section-title">(title)</h3>
      if builds.is_empty() {
        <div class="empty compact-empty">
          <div class="empty-title">(empty)</div>
        </div>
      } else {
        <div class="table-wrap compact-table-wrap">
          <table>
            <thead>
              <tr>
                <th>"Job"</th>
                <th>"Status"</th>
                <th>"System"</th>
                <th>"Output"</th>
              </tr>
            </thead>
            <tbody>
              for dep in builds {
                <tr>
                  <td>
                    <a href=(format!("/build/{}", dep.id)) title=(dep.job_name.as_str())>
                      (derivation_name(&dep.job_name))
                    </a>
                  </td>
                  <td>
                    <span class=(format!("badge badge-{}", dep.status_class))>(dep.status_text)</span>
                  </td>
                  <td>(dep.system)</td>
                  <td>
                    if dep.output_path.is_empty() {
                      "-"
                    } else {
                      <code>(dep.output_path)</code>
                    }
                  </td>
                </tr>
              }
            </tbody>
          </table>
        </div>
      }
    </section>
  })
}

#[component]
async fn products_panel(
  build_id: Uuid,
  products: Vec<BuildProduct>,
) -> Result<impl View> {
  Ok(view! {
    <section class="panel build-detail-section">
      <div class="panel-header">
        <h2>"Build Products"</h2>
      </div>
      if products.is_empty() {
        <div class="empty compact-empty">
          <div class="empty-title">"No products recorded."</div>
        </div>
      } else {
        <div class="table-wrap compact-table-wrap">
          <table>
            <thead>
              <tr>
                <th>"Name"</th>
                <th>"Path"</th>
                <th>"Type"</th>
                <th>"Size"</th>
                <th>"SHA-256"</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              for product in products {
                <tr>
                  <td>
                    (product.name) " "
                    if product.is_directory {
                      <span class="badge badge-pending">"dir"</span>
                    }
                  </td>
                  <td><code>(product.path)</code></td>
                  <td>(product.content_type.unwrap_or_else(|| "-".to_owned()))</td>
                  <td>
                    match product.file_size {
                      Some(size) => (format!("{size} B")),
                      None => "-",
                    }
                  </td>
                  <td>
                    match product.sha256_hash {
                      Some(hash) => <code class="hash-short">(hash)</code>,
                      None => "-",
                    }
                  </td>
                  <td>
                    <a
                      href=(format!("/api/v1/builds/{build_id}/products/{}/download", product.id))
                      class="btn btn-small btn-secondary"
                    >
                      "Download"
                    </a>
                  </td>
                </tr>
              }
            </tbody>
          </table>
        </div>
      }
    </section>
  })
}

#[page("/build/{id}/log")]
async fn build_log_page(cx: &Cx) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::Build).await?;
  let state = app_context::<AppState>(cx);
  let id = *path_param::<Id>(cx)?;
  let build = circus_common::repo::builds::get(&state.pool, id)
    .await
    .ok()
    .ok_or_not_found()?;
  circus_common::repo::evaluations::get_visible(
    &state.pool,
    build.evaluation_id,
    viewer.is_admin,
  )
  .await
  .ok()
  .ok_or_not_found()?;
  let lineage = lineage(state, build.evaluation_id).await?;
  let path = build
    .log_path
    .as_deref()
    .filter(|path| !path.is_empty())
    .ok_or_not_found()?;
  let path = crate::routes::canonical_log_file(
    &state.config.logs.log_dir,
    Path::new(path),
  )
  .await
  .ok_or_not_found()?;
  let raw = tokio::fs::read_to_string(path)
    .await
    .ok()
    .ok_or_not_found()?;
  let log = parse_build_log(&raw);
  let build = build_view(&build);
  let title = format!("Log {}", build.job_name);

  Ok(view! {
    document(title: &title, viewer: &viewer,
      <nav class="breadcrumbs">
        <a href="/projects">"Projects"</a>
        <span class="sep">"/"</span>
        <a href=(format!("/project/{}", lineage.project_id))>(lineage.project_name.clone())</a>
        <span class="sep">"/"</span>
        <a href=(format!("/jobset/{}", lineage.jobset_id))>(lineage.jobset_name.clone())</a>
        <span class="sep">"/"</span>
        <a href=(format!("/evaluation/{}", lineage.eval_id))>(lineage.commit_short.clone())</a>
        <span class="sep">"/"</span>
        <a href=(format!("/build/{}", build.id))>(build.job_name.clone())</a>
        <span class="sep">"/"</span>
        <span class="current">"Log"</span>
      </nav>
      log_page(details: &build, log: log)
    )
  })
}

#[component]
async fn log_page(details: &BuildView, log: BuildLogView) -> Result<impl View> {
  let failed = details.status_class == "failed";

  Ok(view! {
    <section class="build-log-hero">
      <div class="build-log-title">
        <div class="build-log-kicker">"Build log"</div>
        <h1>(details.job_name.clone())</h1>
        <div class="build-log-meta">
          <span class=(format!("status status-{}", details.status_class))>(details.status_text.clone())</span>
          <span>(details.system.clone())</span>
          if !details.duration.is_empty() {
            <span>(details.duration.clone())</span>
          }
          <a href=(format!("/build/{}", details.id))>"Build #" (details.id_short.clone())</a>
        </div>
      </div>
      <div class="build-log-facts" aria-label="Build log summary">
        <div class="build-log-fact">
          <strong>(log.summary.visible_lines)</strong>
          <span>"lines"</span>
        </div>
        <div class="build-log-fact">
          <strong>(log.summary.activity_count)</strong>
          <span>"activities"</span>
        </div>
        <div class="build-log-fact build-log-fact-error">
          <strong>(log.summary.error_count)</strong>
          <span>"errors"</span>
        </div>
        <div class="build-log-fact build-log-fact-warn">
          <strong>(log.summary.warning_count)</strong>
          <span>"warnings"</span>
        </div>
      </div>
    </section>
    <div class="log-layout">
      <aside class="log-activities" aria-label="Build failure summary">
        <div class="log-pane-header">
          <h2>(if failed { "Failure" } else { "Activity" })</h2>
          <span>(log.summary.activity_count) " activities"</span>
        </div>
        if log.activities.is_empty() {
          <div class="empty compact-empty">
            <div class="empty-title">"No Nix activity events."</div>
          </div>
        } else {
          if failed {
            for activity in log.activities.iter().filter(|activity| activity.is_failed()) {
              <section class="failure-activity">
                <div class="failure-title">(activity.label.clone())</div>
                <div class="failure-meta">
                  <span>(activity.kind())</span>
                  if !activity.line_range.is_empty() {
                    <span>(activity.line_range.clone())</span>
                  }
                  if !activity.detail.is_empty() {
                    <span>(activity.detail.clone())</span>
                  }
                </div>
              </section>
            }
            <div class="activity-context-title">"Context"</div>
          }
          <ol class="activity-list">
            for activity in log.activities.iter().filter(|activity| !activity.is_failed()) {
              <li
                class=(format!("activity-item activity-{}", activity.status_class()))
                style=(format!("--activity-depth: {}", activity.depth))
              >
                <span class="activity-dot"></span>
                <div class="activity-body">
                  <span class="activity-label">(activity.label.clone())</span>
                  <div class="activity-meta">
                    <span>(activity.kind())</span>
                    <span>(activity.status())</span>
                    if !activity.detail.is_empty() {
                      <span>(activity.detail.clone())</span>
                    }
                  </div>
                </div>
              </li>
            }
          </ol>
        }
      </aside>
      <section class="log-stream" aria-label="Build log lines">
        <div class="log-pane-header">
          <h2>"Log Stream"</h2>
          <span>"Nix output"</span>
        </div>
        if log.lines.is_empty() {
          <div class="empty compact-empty">
            <div class="empty-title">"No displayable log lines."</div>
          </div>
        } else {
          <ol class="log-lines">
            for line in &log.lines {
              if line.is_phase() {
                <li class="log-line log-line-phase">
                  <span class="log-line-number">(line.number)</span>
                  <span class="log-phase-label">(line.text.clone())</span>
                </li>
              } else {
                <li class=(format!("log-line log-line-{}", line.level_class()))>
                  <span class="log-line-number">(line.number)</span>
                  <span class="log-line-text">(line.text.clone())</span>
                </li>
              }
            }
          </ol>
        }
      </section>
    </div>
  })
}

#[cfg(test)]
mod tests {
  use super::BuildFilterParams;

  #[test]
  fn blank_filter_params_deserialize_to_none() {
    let params = serde_urlencoded::from_str::<BuildFilterParams>(
      "offset=50&limit=50&status=&system=&job_name=",
    )
    .expect("deserialize query");
    assert_eq!(params.status, None);
    assert_eq!(params.system, None);
    assert_eq!(params.job_name, None);
    assert_eq!(params.offset, Some(50));

    let kept = serde_urlencoded::from_str::<BuildFilterParams>("status=failed")
      .expect("deserialize query");
    assert_eq!(kept.status.as_deref(), Some("failed"));
  }
}
