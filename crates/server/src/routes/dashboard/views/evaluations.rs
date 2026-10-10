//! Evaluation and jobset pages.

use std::{
  collections::{BTreeMap, HashMap},
  time::Duration,
};

use circus_common::models::{
  Build,
  BuildStatus,
  Evaluation,
  EvaluationStatus,
  Jobset,
  JobsetState,
  JobsetTriggerMode,
  Project,
};
use jiff::Timestamp;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{error::not_found, page, path_param, query_params},
  runtime::{connected, shard},
  view::{View, component, emit, live, view},
};
use url::form_urlencoded;
use uuid::Uuid;

use super::super::{
  components::{confirm_button, local_time},
  layout::{document, shard_viewer, viewer},
  pages::{
    format_elapsed,
    is_failed_derivation_status,
    is_failed_status,
    is_job_name,
  },
  shared::{
    BuildView,
    DashboardContext,
    DashboardPage,
    DiagnosticSegment,
    EvalProgressView,
    EvalSummaryView,
    EvalView,
    JobStatusCell,
    JobStatusColumn,
    JobStatusRow,
    Pagination,
    build_view,
    commit_url,
    eval_badge,
    eval_progress,
    eval_running_since,
    eval_view,
    eval_view_with_context,
    format_duration,
    status_badge,
  },
};
use crate::{routes::declarative::project_is_mutable, state::AppState};

/// Running evaluations tick their elapsed time every second and reload from
/// the database every third tick.
const TICK: Duration = Duration::from_secs(1);
const RELOAD_EVERY: u32 = 3;

path_param!(id: Uuid, error = not_found);

fn short_commit(hash: &str) -> String {
  hash.get(..12).unwrap_or(hash).to_owned()
}

/// A view-model timestamp in the viewer's timezone, or `-` when unset.
#[component]
async fn when(iso: &str, text: &str) -> Result<impl View> {
  Ok(view! {
    if iso.is_empty() {
      "-"
    } else {
      match iso.parse::<Timestamp>() {
        Ok(at) => local_time(at: at),
        Err(_) => (text),
      }
    }
  })
}

fn non_empty(value: Option<&String>) -> Option<&str> {
  value.map(String::as_str).filter(|value| !value.is_empty())
}

fn elapsed_since(epoch: i64) -> String {
  format_elapsed((Timestamp::now().as_second() - epoch).max(0))
}

#[component]
async fn status_badges(
  class: &str,
  text: &str,
  hidden: bool,
) -> Result<impl View> {
  Ok(view! {
    <span class=(format!("badge badge-{class}"))>(text)</span>
    if hidden {
      <span class="badge badge-skipped">"Hidden"</span>
    }
  })
}

#[component]
async fn commit_cell(
  id: Uuid,
  short: &str,
  subject: &str,
  url: Option<&str>,
) -> Result<impl View> {
  Ok(view! {
    <td class="commit-cell">
      <a href=(format!("/evaluation/{id}"))>(short)</a>
      if !subject.is_empty() {
        match url {
          Some(url) => <a class="commit-subject" href=(url) rel="noopener noreferrer" title=(subject)>(subject)</a>,
          None => <span class="commit-subject" title=(subject)>(subject)</span>,
        }
      }
    </td>
  })
}

#[component]
async fn visibility_form(
  id: Uuid,
  hidden: bool,
  return_to: &str,
  csrf_token: &str,
  label: &str,
) -> Result<impl View> {
  let (value, verb) = if hidden {
    ("false", "Unhide")
  } else {
    ("true", "Hide")
  };

  Ok(view! {
    <form class="inline" method="POST" action=(format!("/evaluation/{id}/visibility"))>
      <input type="hidden" name="csrf_token" value=(csrf_token)>
      <input type="hidden" name="return_to" value=(return_to)>
      <input type="hidden" name="hidden" value=(value)>
      <button class="btn btn-small btn-secondary" type="submit">(verb) (label)</button>
    </form>
  })
}

#[component]
async fn progress_bar(progress: &EvalProgressView) -> Result<impl View> {
  Ok(view! {
    <span class="eval-progress">
      <span class="eval-progress-bar">
        <span style=(format!("width: {}%", progress.percent))></span>
      </span>
      <span class="eval-progress-count">(progress.count.clone())</span>
    </span>
  })
}

/// The duration cell of an evaluation row. A running evaluation keeps
/// counting and polling its progress until it finishes.
#[shard]
async fn eval_duration(
  cx: &Cx,
  id: String,
  duration: String,
) -> Result<impl View> {
  let mut include_hidden = duration_viewer(cx).await?.is_admin;
  let state = app_context::<AppState>(cx);
  let id = id.parse::<Uuid>().map_err(|_| not_found())?;

  Ok(live! {
    let mut eval = circus_common::repo::evaluations::get_visible(&state.pool, id, include_hidden).await.ok();
    let mut tick = 0u32;

    loop {
      let running_since = eval.as_ref().and_then(eval_running_since);
      let progress = eval.as_ref().and_then(eval_progress);
      let finished = eval.as_ref().map_or_else(
        || duration.clone(),
        |eval| format_duration(eval.started_at.as_ref(), eval.finished_at.as_ref()),
      );

      let token = emit! {
        match running_since {
          Some(since) => {
            <span>(elapsed_since(since))</span>
            if let Some(progress) = progress.as_ref() {
              progress_bar(progress: progress)
            }
          },
          None => if finished.is_empty() { "-" } else { (finished) },
        }
      }?;

      if running_since.is_none() || !connected(cx) {
        break Ok(token);
      }

      tokio::time::sleep(TICK).await;
      tick += 1;
      if tick.is_multiple_of(RELOAD_EVERY) {
        include_hidden = duration_viewer(cx).await?.is_admin;
        eval = circus_common::repo::evaluations::get_visible(&state.pool, id, include_hidden).await.ok();
      }
    }
  })
}

/// Both the evaluation list and the jobset page render the duration cell.
async fn duration_viewer(cx: &Cx) -> Result<DashboardContext> {
  match shard_viewer(cx, DashboardPage::Evaluations).await {
    Ok(ctx) => Ok(ctx),
    Err(_) => shard_viewer(cx, DashboardPage::Jobset).await,
  }
}

#[component]
async fn duration_cell(
  id: Uuid,
  running: bool,
  duration: &str,
) -> Result<impl View> {
  Ok(view! {
    <td>
      if running {
        eval_duration(id: id.to_string(), duration: duration.to_owned())
      } else if duration.is_empty() {
        "-"
      } else {
        (duration)
      }
    </td>
  })
}

#[query_params(error = bad_request)]
struct EvaluationsQuery {
  #[serde(
    default,
    deserialize_with = "crate::routes::serde_util::empty_string_as_none"
  )]
  project: Option<String>,
  #[serde(
    default,
    deserialize_with = "crate::routes::serde_util::empty_string_as_none"
  )]
  jobset:  Option<String>,
  #[serde(
    default,
    deserialize_with = "crate::routes::serde_util::empty_string_as_none"
  )]
  commit:  Option<String>,
  #[serde(
    default,
    deserialize_with = "crate::routes::serde_util::empty_string_as_none"
  )]
  status:  Option<String>,
  limit:   Option<i64>,
  offset:  Option<i64>,
}

const EVAL_STATUSES: &[(&str, &str)] = &[
  ("pending", "Pending"),
  ("running", "Running"),
  ("completed", "Completed"),
  ("failed", "Failed"),
  ("cancelled", "Cancelled"),
  ("timed_out", "Timed out"),
];

async fn load_evaluations(
  state: &AppState,
  query: &EvaluationsQuery,
  include_hidden: bool,
  limit: i64,
  offset: i64,
) -> (Vec<EvalView>, i64) {
  let filter = circus_common::repo::evaluations::EvaluationListFilter {
    project: non_empty(query.project.as_ref()),
    jobset: non_empty(query.jobset.as_ref()),
    commit: non_empty(query.commit.as_ref()),
    status: non_empty(query.status.as_ref()),
    include_hidden,
  };
  let items = circus_common::repo::evaluations::list_page_filtered(
    &state.pool,
    filter,
    limit,
    offset,
  )
  .await
  .unwrap_or_default();
  let total =
    circus_common::repo::evaluations::count_page_filtered(&state.pool, filter)
      .await
      .unwrap_or(0);

  let mut evals = Vec::new();
  for e in &items {
    let (jobset_name, project) =
      match circus_common::repo::jobsets::get(&state.pool, e.jobset_id).await {
        Ok(jobset) => {
          let project =
            circus_common::repo::projects::get(&state.pool, jobset.project_id)
              .await
              .ok();
          (jobset.name, project)
        },
        Err(_) => ("-".to_owned(), None),
      };
    let (project_name, repository_url) = project.map_or_else(
      || ("-".to_owned(), None),
      |project| (project.name, Some(project.repository_url)),
    );
    evals.push(eval_view_with_context(
      e,
      &jobset_name,
      &project_name,
      repository_url.as_deref(),
    ));
  }

  (evals, total)
}

#[page("/evaluations")]
async fn evaluations_page(cx: &Cx) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::Evaluations).await?;
  let state = app_context::<AppState>(cx);
  let query = query_params::<EvaluationsQuery>(cx)?;
  let limit = query.limit.unwrap_or(50).clamp(1, 200);
  let offset = query.offset.unwrap_or(0).max(0);
  let (evals, total) =
    load_evaluations(state, query, viewer.is_admin, limit, offset).await;
  let pagination = Pagination::new(total, offset, limit);

  let field = |value: &Option<String>| value.clone().unwrap_or_default();
  let status = field(&query.status);
  let project = field(&query.project);
  let jobset = field(&query.jobset);
  let commit = field(&query.commit);
  let page_href = |offset: i64| {
    let query = form_urlencoded::Serializer::new(String::new())
      .append_pair("offset", &offset.to_string())
      .append_pair("limit", &limit.to_string())
      .append_pair("status", &status)
      .append_pair("project", &project)
      .append_pair("jobset", &jobset)
      .append_pair("commit", &commit)
      .finish();
    format!("/evaluations?{query}")
  };
  let prev_href = page_href(pagination.prev_offset);
  let next_href = page_href(pagination.next_offset);

  Ok(view! {
    document(title: "Evaluations", viewer: &viewer,
      <nav class="breadcrumbs">
        <a href="/">"Home"</a>
        <span class="sep">"/"</span>
        <span class="current">"Evaluations"</span>
      </nav>
      <h1>"Evaluations"</h1>

      <form method="get" action="/evaluations" class="filter-form">
        <label>
          "Status: "
          <select name="status">
            <option value="">"All"</option>
            for (value, label) in EVAL_STATUSES.iter().copied() {
              <option value=(value) selected=(status == value)>(label)</option>
            }
          </select>
        </label>
        <label>"Project: " <input type="text" name="project" value=(project.clone()) placeholder="project name"></label>
        <label>"Jobset: " <input type="text" name="jobset" value=(jobset.clone()) placeholder="jobset name"></label>
        <label>"Commit: " <input type="text" name="commit" value=(commit.clone()) placeholder="commit hash"></label>
        <button type="submit" class="btn btn-secondary">"Filter"</button>
      </form>

      if evals.is_empty() {
        <div class="empty">
          <div class="empty-title">"No evaluations yet"</div>
          <div class="empty-hint">"Evaluations will appear here once a jobset is evaluated."</div>
        </div>
      } else {
        <div class="table-wrap">
          <table>
            <colgroup>
              <col style="width:22%">
              <col style="width:10%">
              <col style="width:10%">
              <col style="width:10%">
              <col style="width:12%">
              <col style="width:12%">
              <col style="width:15%">
              if viewer.is_admin {
                <col style="width:9%">
              }
            </colgroup>
            <thead>
              <tr>
                <th>"Commit"</th>
                <th>"Project"</th>
                <th>"Jobset"</th>
                <th>"Status"</th>
                <th>"Started"</th>
                <th>"Finished"</th>
                <th>"Duration"</th>
                if viewer.is_admin {
                  <th>"Actions"</th>
                }
              </tr>
            </thead>
            <tbody>
              #[key(e.id.as_u128())]
              for e in evals {
                <tr>
                  commit_cell(
                    id: e.id,
                    short: &e.commit_short,
                    subject: &e.commit_subject,
                    url: e.commit_url.as_deref(),
                  )
                  <td>(e.project_name.clone())</td>
                  <td>(e.jobset_name.clone())</td>
                  <td>status_badges(class: &e.status_class, text: &e.status_text, hidden: e.hidden)</td>
                  <td>when(iso: &e.started_iso, text: &e.started)</td>
                  <td>when(iso: &e.finished_iso, text: &e.finished)</td>
                  duration_cell(id: e.id, running: e.running_since.is_some(), duration: &e.duration)
                  if viewer.is_admin {
                    <td class="row-actions">
                      visibility_form(
                        id: e.id,
                        hidden: e.hidden,
                        return_to: "/evaluations",
                        csrf_token: &viewer.csrf_token,
                        label: "",
                      )
                    </td>
                  }
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

struct EvaluationData {
  eval:               EvalView,
  status:             EvaluationStatus,
  builds:             Vec<BuildView>,
  failed_derivations: Vec<BuildView>,
  project:            Project,
  jobset:             Jobset,
  succeeded:          usize,
  failed:             usize,
  running:            usize,
  pending:            usize,
}

async fn load_evaluation(
  state: &AppState,
  id: Uuid,
  include_hidden: bool,
) -> Option<EvaluationData> {
  let eval = circus_common::repo::evaluations::get_visible(
    &state.pool,
    id,
    include_hidden,
  )
  .await
  .ok()?;
  let jobset = circus_common::repo::jobsets::get(&state.pool, eval.jobset_id)
    .await
    .ok()?;
  let project =
    circus_common::repo::projects::get(&state.pool, jobset.project_id)
      .await
      .ok()?;
  let builds =
    circus_common::repo::builds::list_for_evaluation(&state.pool, id)
      .await
      .unwrap_or_default();

  let top_level: Vec<&Build> = builds
    .iter()
    .filter(|build| is_job_name(&build.job_name))
    .collect();
  let count = |matches: fn(BuildStatus) -> bool| {
    top_level
      .iter()
      .filter(|build| matches(build.status))
      .count()
  };

  let view = EvalView {
    commit_url: commit_url(&project.repository_url, &eval.commit_hash),
    ..eval_view(&eval)
  };

  Some(EvaluationData {
    succeeded: count(|status| status == BuildStatus::Succeeded),
    failed: count(is_failed_status),
    running: count(|status| status == BuildStatus::Running),
    pending: count(|status| status == BuildStatus::Pending),
    failed_derivations: builds
      .iter()
      .filter(|build| is_failed_derivation_status(build.status))
      .map(build_view)
      .collect(),
    builds: top_level.into_iter().map(build_view).collect(),
    status: eval.status,
    eval: view,
    project,
    jobset,
  })
}

#[page("/evaluation/{id}")]
async fn evaluation_page(cx: &Cx) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::Evaluation).await?;
  let state = app_context::<AppState>(cx);
  let id = *path_param::<Id>(cx)?;
  let data = load_evaluation(state, id, viewer.is_admin)
    .await
    .ok_or_else(not_found)?;
  let title = format!("Evaluation {}", data.eval.commit_short);

  Ok(view! {
    document(title: &title, viewer: &viewer,
      <nav class="breadcrumbs">
        <a href="/projects">"Projects"</a>
        <span class="sep">"/"</span>
        <a href=(format!("/project/{}", data.project.id))>(data.project.name.clone())</a>
        <span class="sep">"/"</span>
        <a href=(format!("/project/{}#jobsets", data.project.id))>"Jobsets"</a>
        <span class="sep">"/"</span>
        <a href=(format!("/jobset/{}", data.jobset.id))>(data.jobset.name.clone())</a>
        <span class="sep">"/"</span>
        <span class="current">(data.eval.commit_short.clone())</span>
      </nav>
      <h1>"Evaluation " (data.eval.commit_short.clone())</h1>
      evaluation_body(id: id.to_string())
    )
  })
}

/// Everything below the heading. While the evaluation runs it ticks and
/// reloads, and its final render shows the finished builds.
#[shard]
async fn evaluation_body(cx: &Cx, id: String) -> Result<impl View> {
  let mut ctx = shard_viewer(cx, DashboardPage::Evaluation).await?;
  let state = app_context::<AppState>(cx);
  let id = id.parse::<Uuid>().map_err(|_| not_found())?;
  let first = load_evaluation(state, id, ctx.is_admin)
    .await
    .ok_or_else(not_found)?;

  Ok(live! {
    let mut data = first;
    let mut tick = 0u32;

    loop {
      let running = data.status == EvaluationStatus::Running;
      let token = emit! {
        evaluation_details(data: &data, is_admin: ctx.is_admin, csrf_token: &ctx.csrf_token)
      }?;

      if !running || !connected(cx) {
        break Ok(token);
      }

      tokio::time::sleep(TICK).await;
      tick += 1;
      if tick.is_multiple_of(RELOAD_EVERY) {
        ctx = shard_viewer(cx, DashboardPage::Evaluation).await?;
        if let Some(fresh) = load_evaluation(state, id, ctx.is_admin).await {
          data = fresh;
        }
      }
    }
  })
}

#[component]
async fn evaluation_details(
  data: &EvaluationData,
  is_admin: bool,
  csrf_token: &str,
) -> Result<impl View> {
  let eval = &data.eval;
  let can_cancel = matches!(eval.status_class.as_str(), "running" | "pending");
  let can_restart = matches!(
    eval.status_class.as_str(),
    "failed" | "cancelled" | "timed-out"
  );
  let return_to = format!("/evaluation/{}", eval.id);

  Ok(view! {
    if is_admin {
      <div class="page-actions">
        visibility_form(
          id: eval.id,
          hidden: eval.hidden,
          return_to: &return_to,
          csrf_token: csrf_token,
          label: "Evaluation",
        )
        if can_cancel {
          <form class="inline" method="POST" action=(format!("/evaluation/{}/cancel", eval.id))>
            <input type="hidden" name="csrf_token" value=(csrf_token)>
            confirm_button(prompt: "Cancel this evaluation?", class: "btn btn-small btn-danger", label: "Cancel Evaluation")
          </form>
        }
        if can_restart {
          <form class="inline" method="POST" action=(format!("/evaluation/{}/restart", eval.id))>
            <input type="hidden" name="csrf_token" value=(csrf_token)>
            confirm_button(
              prompt: "Restart this evaluation and discard its current builds?",
              class: "btn btn-small btn-secondary",
              label: "Restart Evaluation",
            )
          </form>
        }
      </div>
    }
    <div class="build-meta">
      <div class="build-meta-item">
        <span class="build-meta-label">"Status"</span>
        <span class="build-meta-value">
          status_badges(class: &eval.status_class, text: &eval.status_text, hidden: eval.hidden)
        </span>
      </div>
      <div class="build-meta-item">
        <span class="build-meta-label">"Queued"</span>
        <span class="build-meta-value">
          when(iso: &eval.time_iso, text: &eval.time)
        </span>
      </div>
      <div class="build-meta-item">
        <span class="build-meta-label">"Started"</span>
        <span class="build-meta-value">
          when(iso: &eval.started_iso, text: &eval.started)
        </span>
      </div>
      match eval.running_since {
        Some(since) => <div class="build-meta-item">
          <span class="build-meta-label">"Elapsed"</span>
          <span class="build-meta-value">(elapsed_since(since))</span>
        </div>,
        None => if !eval.duration.is_empty() {
          <div class="build-meta-item">
            <span class="build-meta-label">"Duration"</span>
            <span class="build-meta-value">(eval.duration.clone())</span>
          </div>
        },
      }
      if let Some(progress) = eval.progress.as_ref() {
        <div class="build-meta-item">
          <span class="build-meta-label">"Attributes"</span>
          <span class="build-meta-value eval-progress">
            <span class="eval-progress-count">(progress.count.clone())</span>
            <span class="eval-progress-bar">
              <span style=(format!("width: {}%", progress.percent))></span>
            </span>
          </span>
        </div>
      }
      <div class="build-meta-item build-meta-wide">
        <span class="build-meta-label">"Commit"</span>
        <span class="build-meta-value">
          match eval.commit_url.as_deref() {
            Some(url) => <a class="commit-link" href=(url) rel="noopener noreferrer"><code>(eval.commit_hash.clone())</code></a>,
            None => <code>(eval.commit_hash.clone())</code>,
          }
          if !eval.commit_subject.is_empty() {
            <span class="commit-message">(eval.commit_subject.clone())</span>
          }
        </span>
      </div>
      if let Some(replacement) = eval.superseded_by {
        <div class="build-meta-item">
          <span class="build-meta-label">"Superseded by"</span>
          <span class="build-meta-value">
            <a href=(format!("/evaluation/{replacement}"))>"newer evaluation"</a>
          </span>
        </div>
      }
    </div>
    if !eval.error_message.is_empty() {
      <section class="evaluation-diagnostics">
        <div class="section-heading">
          <h2>"Evaluation Error"</h2>
          <span>"Nix diagnostics"</span>
        </div>
        <pre class="error-detail">
          for segment in eval.error_segments.iter() {
            diagnostic_segment(segment: segment)
          }
        </pre>
      </section>
    }

    <section class="outcome-panel">
      <div class="section-heading">
        <h2>"Build Outcomes"</h2>
        <span>"This evaluation"</span>
      </div>
      <div class="outcome-grid">
        outcome_card(class: "outcome-success", count: data.succeeded, label: "Succeeded")
        outcome_card(class: "outcome-failed", count: data.failed, label: "Failed")
        outcome_card(class: "outcome-running", count: data.running, label: "Running")
        outcome_card(class: "outcome-pending", count: data.pending, label: "Pending")
      </div>
    </section>

    if !data.failed_derivations.is_empty() {
      <h2>"Failed Derivations"</h2>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>"Derivation"</th>
              <th>"Status"</th>
              <th>"System"</th>
            </tr>
          </thead>
          <tbody>
            for b in data.failed_derivations.iter() {
              <tr>
                <td><a href=(format!("/build/{}", b.id))>(b.job_name.clone())</a></td>
                <td><span class=(format!("badge badge-{}", b.status_class))>(b.status_text.clone())</span></td>
                <td>(b.system.clone())</td>
              </tr>
            }
          </tbody>
        </table>
      </div>
    }

    <h2>"Builds"</h2>
    if data.builds.is_empty() {
      <div class="empty">"No builds for this evaluation."</div>
    } else {
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th>"Job"</th>
              <th>"Status"</th>
              <th>"System"</th>
              <th>"Created"</th>
            </tr>
          </thead>
          <tbody>
            for b in data.builds.iter() {
              <tr>
                <td><a href=(format!("/build/{}", b.id))>(b.job_name.clone())</a></td>
                <td><span class=(format!("badge badge-{}", b.status_class))>(b.status_text.clone())</span></td>
                <td>(b.system.clone())</td>
                <td>(b.created_at.clone())</td>
              </tr>
            }
          </tbody>
        </table>
      </div>
    }
  })
}

#[component]
async fn diagnostic_segment(segment: &DiagnosticSegment) -> Result<impl View> {
  Ok(view! {
    <span class=(segment.class.as_str())>(segment.text.as_str())</span>
  })
}

#[component]
async fn outcome_card(
  class: &str,
  count: usize,
  label: &str,
) -> Result<impl View> {
  Ok(view! {
    <div class=(format!("outcome-card {class}"))>
      <div class="stat-value">(count)</div>
      <div class="stat-label">(label)</div>
    </div>
  })
}

async fn recent_evaluations(
  state: &AppState,
  jobset: Uuid,
  include_hidden: bool,
) -> (Vec<Evaluation>, Vec<Build>) {
  let evals = circus_common::repo::evaluations::list_filtered_with_visibility(
    &state.pool,
    Some(jobset),
    None,
    20,
    0,
    include_hidden,
  )
  .await
  .unwrap_or_default();
  let eval_ids: Vec<Uuid> = evals.iter().map(|e| e.id).collect();
  let builds = circus_common::repo::builds::list_for_jobset_evaluations(
    &state.pool,
    jobset,
    &eval_ids,
  )
  .await
  .unwrap_or_default();
  (evals, builds)
}

async fn jobset_and_project(
  state: &AppState,
  id: Uuid,
) -> Option<(Jobset, Project)> {
  let jobset = circus_common::repo::jobsets::get(&state.pool, id)
    .await
    .ok()?;
  let project =
    circus_common::repo::projects::get(&state.pool, jobset.project_id)
      .await
      .ok()?;
  Some((jobset, project))
}

fn eval_summaries(
  evals: &[Evaluation],
  builds: &[Build],
) -> Vec<EvalSummaryView> {
  let mut builds_by_eval: HashMap<Uuid, Vec<&Build>> = HashMap::new();
  for build in builds.iter().filter(|build| is_job_name(&build.job_name)) {
    builds_by_eval
      .entry(build.evaluation_id)
      .or_default()
      .push(build);
  }

  evals
    .iter()
    .map(|e| {
      let (status_text, status_class) = eval_badge(&e.status);
      let eval_builds = builds_by_eval
        .get(&e.id)
        .map_or_else(|| &[][..], Vec::as_slice);
      let count = |matches: fn(BuildStatus) -> bool| {
        eval_builds
          .iter()
          .filter(|build| matches(build.status))
          .count() as i64
      };

      EvalSummaryView {
        id: e.id,
        commit_short: short_commit(&e.commit_hash),
        commit_subject: e.commit_subject.clone().unwrap_or_default(),
        status_text,
        status_class,
        time: e.evaluation_time.strftime("%Y-%m-%d %H:%M UTC").to_string(),
        time_iso: e.evaluation_time.to_string(),
        duration: format_duration(
          e.started_at.as_ref(),
          e.finished_at.as_ref(),
        ),
        running_since: eval_running_since(e),
        succeeded: count(|status| status == BuildStatus::Succeeded),
        failed: count(|status| {
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
        }),
        pending: count(|status| status == BuildStatus::Pending),
        hidden: e.hidden,
      }
    })
    .collect()
}

#[component]
async fn jobset_breadcrumbs(
  project: &Project,
  jobset: &Jobset,
  current: Option<&str>,
) -> Result<impl View> {
  Ok(view! {
    <nav class="breadcrumbs">
      <a href="/projects">"Projects"</a>
      <span class="sep">"/"</span>
      <a href=(format!("/project/{}", project.id))>(project.name.clone())</a>
      <span class="sep">"/"</span>
      <a href=(format!("/project/{}#jobsets", project.id))>"Jobsets"</a>
      <span class="sep">"/"</span>
      match current {
        Some(current) => {
          <a href=(format!("/jobset/{}", jobset.id))>(jobset.name.clone())</a>
          <span class="sep">"/"</span>
          <span class="current">(current)</span>
        },
        None => <span class="current">(jobset.name.clone())</span>,
      }
    </nav>
  })
}

#[component]
async fn jobset_tabs(id: Uuid, jobs: bool) -> Result<impl View> {
  Ok(view! {
    <nav class="tab-nav">
      <a class=(if jobs { "" } else { "active" }) href=(format!("/jobset/{id}"))>"Evaluations"</a>
      <a class=(if jobs { "active" } else { "" }) href=(format!("/jobset/{id}/jobs"))>"Jobs"</a>
    </nav>
  })
}

#[page("/jobset/{id}")]
async fn jobset_page(cx: &Cx) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::Jobset).await?;
  let state = app_context::<AppState>(cx);
  let id = *path_param::<Id>(cx)?;
  let (jobset, project) =
    jobset_and_project(state, id).await.ok_or_else(not_found)?;
  let (evals, builds) = recent_evaluations(state, id, viewer.is_admin).await;
  let summaries = eval_summaries(&evals, &builds);
  let mutable = project_is_mutable(state, &project);
  let return_to = format!("/jobset/{}", jobset.id);

  Ok(view! {
    document(title: &jobset.name, viewer: &viewer,
      jobset_breadcrumbs(project: &project, jobset: &jobset, current: None)
      <h1>(jobset.name.clone())</h1>

      jobset_tabs(id: jobset.id, jobs: false)

      if project.managed_declaratively && !mutable {
        <p class="flash-message">"Managed by declarative configuration. Runtime changes are disabled."</p>
      }

      if viewer.is_admin && mutable {
        <div class="page-actions">
          <form class="inline" method="POST" action=(format!("/jobset/{}/delete", jobset.id))>
            <input type="hidden" name="csrf_token" value=(viewer.csrf_token.as_str())>
            confirm_button(
              prompt: "Delete this jobset and all of its evaluations/builds?",
              class: "btn btn-danger btn-small",
              label: "Delete Jobset",
            )
          </form>
        </div>
      }

      jobset_details(jobset: &jobset)

      if let Some(latest) = summaries.first() {
        <h2>"Latest Evaluation"</h2>
        <div class="stats-grid">
          stat_card(count: latest.succeeded, label: "Succeeded")
          stat_card(count: latest.failed, label: "Failed")
          stat_card(count: latest.pending, label: "Pending")
        </div>
      }

      <h2>"Recent Evaluations"</h2>
      if summaries.is_empty() {
        <div class="empty">
          <div class="empty-title">"No evaluations yet"</div>
          <div class="empty-hint">
            "The evaluator will schedule this jobset based on its trigger mode and check interval."
          </div>
        </div>
      } else {
        <div class="table-wrap">
          <table>
            <colgroup>
              <col style="width:28%">
              <col style="width:12%">
              <col style="width:9%">
              <col style="width:9%">
              <col style="width:9%">
              <col style="width:9%">
              <col style="width:14%">
              if viewer.is_admin {
                <col style="width:10%">
              }
            </colgroup>
            <thead>
              <tr>
                <th>"Commit"</th>
                <th>"Status"</th>
                <th>"Succeeded"</th>
                <th>"Failed"</th>
                <th>"Pending"</th>
                <th>"Duration"</th>
                <th>"Time"</th>
                if viewer.is_admin {
                  <th>"Actions"</th>
                }
              </tr>
            </thead>
            <tbody>
              #[key(e.id.as_u128())]
              for e in summaries.iter() {
                <tr>
                  commit_cell(id: e.id, short: &e.commit_short, subject: &e.commit_subject, url: None)
                  <td>status_badges(class: &e.status_class, text: &e.status_text, hidden: e.hidden)</td>
                  <td>(e.succeeded)</td>
                  <td>(e.failed)</td>
                  <td>(e.pending)</td>
                  duration_cell(id: e.id, running: e.running_since.is_some(), duration: &e.duration)
                  <td>when(iso: &e.time_iso, text: &e.time)</td>
                  if viewer.is_admin {
                    <td>
                      visibility_form(
                        id: e.id,
                        hidden: e.hidden,
                        return_to: &return_to,
                        csrf_token: &viewer.csrf_token,
                        label: "",
                      )
                    </td>
                  }
                </tr>
              }
            </tbody>
          </table>
        </div>
      }
    )
  })
}

#[component]
async fn stat_card(count: i64, label: &str) -> Result<impl View> {
  Ok(view! {
    <div class="stat-card">
      <div class="stat-value">(count)</div>
      <div class="stat-label">(label)</div>
    </div>
  })
}

#[component]
async fn jobset_details(jobset: &Jobset) -> Result<impl View> {
  let (state_class, state_text) = match jobset.state {
    JobsetState::Disabled => ("cancelled", "Disabled"),
    JobsetState::Enabled => ("completed", "Enabled"),
    JobsetState::OneShot => ("pending", "One-Shot"),
    JobsetState::OneAtATime => ("running", "One-at-a-Time"),
  };
  let trigger = match jobset.trigger_mode {
    JobsetTriggerMode::SourceChange => "Source change",
    JobsetTriggerMode::Interval => "Interval rebuild",
  };
  let yes_no = |value: bool| if value { "Yes" } else { "No" };

  Ok(view! {
    <dl class="detail-grid">
      <dt>"Expression"</dt>
      <dd><code>(jobset.nix_expression.clone())</code></dd>
      <dt>"Flake mode"</dt>
      <dd>(yes_no(jobset.flake_mode))</dd>
      <dt>"State"</dt>
      <dd><span class=(format!("badge badge-{state_class}"))>(state_text)</span></dd>
      <dt>"Trigger mode"</dt>
      <dd>(trigger)</dd>
      <dt>"Check interval"</dt>
      <dd>(jobset.check_interval) "s"</dd>
      <dt>"Only build latest"</dt>
      <dd>(yes_no(jobset.only_build_latest))</dd>
      if !jobset.path_filters.is_empty() {
        <dt>"Path filters"</dt>
        <dd>
          <ul>
            for path_filter in jobset.path_filters.iter() {
              <li><code>(path_filter.clone())</code></li>
            }
          </ul>
        </dd>
      }
      if let Some(systems) = jobset.systems.as_ref() {
        <dt>"Systems"</dt>
        <dd>(systems.join(", "))</dd>
      }
      <dt>"Last checked"</dt>
      <dd>
        match jobset.last_checked_at {
          Some(at) => (at.strftime("%Y-%m-%d %H:%M:%S").to_string()),
          None => "Never",
        }
      </dd>
    </dl>
  })
}

#[query_params(error = bad_request)]
struct JobsQuery {
  show_inactive: Option<String>,
  #[serde(
    default,
    deserialize_with = "crate::routes::serde_util::empty_string_as_none"
  )]
  q:             Option<String>,
}

impl JobsQuery {
  fn show_inactive(&self) -> bool {
    self
      .show_inactive
      .as_deref()
      .is_some_and(|value| matches!(value, "1" | "true" | "yes" | "on"))
  }
}

fn job_history(
  evals: &[Evaluation],
  builds: Vec<Build>,
  show_inactive: bool,
  search: &str,
) -> (Vec<JobStatusColumn>, Vec<JobStatusRow>) {
  let latest_eval_id = evals.first().map(|e| e.id);
  let columns: Vec<JobStatusColumn> = evals
    .iter()
    .map(|e| {
      let hidden_suffix = if e.hidden { " (hidden)" } else { "" };
      JobStatusColumn {
        eval_id: e.id,
        label:   e.evaluation_time.strftime("%m-%d %H:%M").to_string(),
        title:   format!("{}{hidden_suffix}", short_commit(&e.commit_hash)),
      }
    })
    .collect();

  let mut builds_by_job: BTreeMap<String, HashMap<Uuid, Build>> =
    BTreeMap::new();
  for build in builds
    .into_iter()
    .filter(|build| is_job_name(&build.job_name))
  {
    builds_by_job
      .entry(build.job_name.clone())
      .or_default()
      .insert(build.evaluation_id, build);
  }

  let search = search.to_lowercase();
  let mut rows = Vec::new();
  for (job_name, by_eval) in builds_by_job {
    let is_active =
      latest_eval_id.is_some_and(|eval_id| by_eval.contains_key(&eval_id));
    if (!show_inactive && !is_active)
      || !job_name.to_lowercase().contains(&search)
    {
      continue;
    }
    let cells = columns
      .iter()
      .map(|column| {
        by_eval.get(&column.eval_id).map_or_else(
          || {
            JobStatusCell {
              href:         String::new(),
              status_text:  "-".to_owned(),
              status_class: "skipped".to_owned(),
            }
          },
          |build| {
            let (status_text, status_class) = status_badge(build.status);
            JobStatusCell {
              href: format!("/build/{}", build.id),
              status_text,
              status_class,
            }
          },
        )
      })
      .collect();
    rows.push(JobStatusRow {
      job_name,
      is_active,
      cells,
    });
  }

  (columns, rows)
}

#[page("/jobset/{id}/jobs")]
async fn jobset_jobs_page(cx: &Cx) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::JobsetJobs).await?;
  let state = app_context::<AppState>(cx);
  let id = *path_param::<Id>(cx)?;
  let query = query_params::<JobsQuery>(cx)?;
  let show_inactive = query.show_inactive();
  let search = query.q.clone().unwrap_or_default();
  let (jobset, project) =
    jobset_and_project(state, id).await.ok_or_else(not_found)?;
  let (evals, builds) = recent_evaluations(state, id, viewer.is_admin).await;
  let (columns, rows) = job_history(&evals, builds, show_inactive, &search);
  let toggle_href = if show_inactive {
    format!("/jobset/{}/jobs", jobset.id)
  } else {
    format!("/jobset/{}/jobs?show_inactive=true", jobset.id)
  };

  let title = format!("{} Jobs", jobset.name);

  Ok(view! {
    document(title: &title, viewer: &viewer,
      jobset_breadcrumbs(project: &project, jobset: &jobset, current: Some("Jobs"))
      <h1>(jobset.name.clone())</h1>

      jobset_tabs(id: jobset.id, jobs: true)

      <div class="job-history-toolbar">
        <form method="get" action=(format!("/jobset/{}/jobs", jobset.id))>
          if show_inactive {
            <input type="hidden" name="show_inactive" value="true">
          }
          <label>
            "Search jobs"
            <input type="search" name="q" value=(search.clone()) placeholder="job name">
          </label>
        </form>
        <a class="btn btn-small btn-secondary" href=(toggle_href)>
          (if show_inactive { "Hide inactive jobs" } else { "Show inactive jobs" })
        </a>
      </div>

      if columns.is_empty() {
        <div class="empty">
          <div class="empty-title">"No evaluations yet"</div>
          <div class="empty-hint">"Job status history will appear after this jobset evaluates."</div>
        </div>
      } else if rows.is_empty() && !search.is_empty() {
        <div class="empty">
          <div class="empty-title">"No matching jobs"</div>
          <div class="empty-hint">"No job name contains that search."</div>
        </div>
      } else if rows.is_empty() {
        <div class="empty">
          <div class="empty-title">"No active jobs"</div>
          <div class="empty-hint">"Use \"Show inactive jobs\" to include jobs missing from the latest evaluation."</div>
        </div>
      } else {
        <div class="table-wrap job-history-wrap">
          <table class="job-history-table">
            <thead>
              <tr>
                <th class="job-history-name">"Job"</th>
                for column in columns.iter() {
                  <th class="job-history-eval" title=(column.title.clone())>
                    <a href=(format!("/evaluation/{}", column.eval_id))>(column.label.clone())</a>
                  </th>
                }
              </tr>
            </thead>
            <tbody>
              for row in rows {
                <tr>
                  <td class="job-history-name">
                    <strong>(row.job_name.clone())</strong>
                    if !row.is_active {
                      <span class="badge badge-skipped">"Inactive"</span>
                    }
                  </td>
                  for cell in row.cells {
                    <td class="job-history-cell">
                      if cell.href.is_empty() {
                        <span class=(format!("badge badge-{}", cell.status_class))>(cell.status_text)</span>
                      } else {
                        <a class=(format!("badge badge-{}", cell.status_class)) href=(cell.href)>(cell.status_text)</a>
                      }
                    </td>
                  }
                </tr>
              }
            </tbody>
          </table>
        </div>
      }
    )
  })
}
