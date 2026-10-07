//! The project page.

use std::cmp::Reverse;

use circus_common::{
  models::{CreateJobset, Evaluation, Jobset, JobsetTriggerMode, Project},
  validate::Validate,
};
use serde::Deserialize;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Form,
    error::{forbidden, not_found, see_other},
    page,
    path_param,
  },
  view::{View, component, view},
};
use uuid::Uuid;

use super::super::{
  components::{confirm_button, local_time},
  layout::{document, viewer},
  shared::{
    DashboardContext,
    DashboardPage,
    EvalView,
    eval_view,
    repository_page_url,
  },
};
use crate::{permissions::Permission, state::AppState};

path_param!(id: Uuid, error = not_found);

/// A rejected Add Jobset post, shown again with why.
struct Rejected {
  form:    JobsetForm,
  message: String,
}

#[derive(Deserialize)]
struct JobsetForm {
  csrf_token:        String,
  name:              String,
  nix_expression:    String,
  flake_mode:        Option<String>,
  trigger_mode:      JobsetTriggerMode,
  only_build_latest: Option<String>,
  #[serde(default)]
  path_filters:      String,
}

#[page("/project/{id}")]
async fn project_page(cx: &Cx) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::Project).await?;
  Ok(view! { project_view(viewer: viewer, rejected: None) })
}

/// Creates a jobset, answering a rejection with the project page so the
/// form keeps what was typed.
#[page(POST "/project/{id}/jobsets")]
async fn jobset_create(
  cx: &Cx,
  Form(form): Form<JobsetForm>,
) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::Project).await?;
  let state = app_context::<AppState>(cx);
  let project_id = *path_param::<Id>(cx)?;

  if viewer.check_csrf(&form.csrf_token).is_err()
    || viewer
      .require_permission(Permission::CreateProjects)
      .is_err()
  {
    return Err(forbidden().into());
  }

  let input = CreateJobset {
    project_id,
    name: form.name.clone(),
    nix_expression: form.nix_expression.clone(),
    enabled: None,
    flake_mode: Some(form.flake_mode.is_some()),
    check_interval: None,
    trigger_mode: Some(form.trigger_mode),
    branch: None,
    branch_pattern: None,
    tag_pattern: None,
    scheduling_shares: None,
    state: None,
    keep_nr: None,
    systems: None,
    only_build_latest: Some(form.only_build_latest.is_some()),
    path_filters: Some(
      form
        .path_filters
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect(),
    ),
  };
  let created = match crate::routes::declarative::require_project_mutable(
    state, project_id,
  )
  .await
  {
    Ok(_) => {
      match input.validate() {
        Ok(()) => {
          circus_common::repo::jobsets::create(&state.pool, input)
            .await
            .map_err(|error| error.to_string())
        },
        Err(message) => Err(message),
      }
    },
    Err(error) => Err(error.0.to_string()),
  };

  match created {
    Ok(_) => Err(see_other(format!("/project/{project_id}")).into()),
    Err(message) => {
      Ok(view! {
        project_view(viewer: viewer, rejected: Some(Rejected { form, message }))
      })
    },
  }
}

#[component]
async fn project_view(
  cx: &Cx,
  viewer: DashboardContext,
  rejected: Option<Rejected>,
) -> Result<impl View> {
  let state = app_context::<AppState>(cx);
  let id = *path_param::<Id>(cx)?;
  let project = circus_common::repo::projects::get(&state.pool, id)
    .await
    .map_err(|_| not_found())?;
  let jobsets =
    circus_common::repo::jobsets::list_for_project(&state.pool, id, 100, 0)
      .await
      .unwrap_or_default();
  let evals = recent_evaluations(state, &jobsets, viewer.is_admin).await;
  let mutable = crate::routes::declarative::project_is_mutable(state, &project);
  let admin = viewer.is_admin;
  let csrf_token = viewer.csrf_token.clone();
  let title = project.name.clone();

  Ok(view! {
    document(title: &title, viewer: &viewer,
      <nav class="breadcrumbs">
        <a href="/projects">"Projects"</a>
        <span class="sep">"/"</span>
        <span class="current">(project.name.clone())</span>
      </nav>
      summary(project: &project, mutable: mutable)
      if admin {
        actions(
          project_id: project.id,
          mutable: mutable,
          csrf_token: &csrf_token,
          rejected: rejected,
        )
      }
      jobsets_panel(
        jobsets: jobsets,
        manage: admin && mutable,
        csrf_token: &csrf_token,
      )
      evaluations_panel(
        evals: evals,
        project_id: project.id,
        admin: admin,
        csrf_token: &csrf_token,
      )
    )
  })
}

/// The ten newest evaluations across the project's jobsets.
async fn recent_evaluations(
  state: &AppState,
  jobsets: &[Jobset],
  include_hidden: bool,
) -> Vec<Evaluation> {
  let mut evals = Vec::new();
  for jobset in jobsets {
    let mut jobset_evals =
      circus_common::repo::evaluations::list_filtered_with_visibility(
        &state.pool,
        Some(jobset.id),
        None,
        5,
        0,
        include_hidden,
      )
      .await
      .unwrap_or_default();
    evals.append(&mut jobset_evals);
  }
  evals.sort_by_key(|e| Reverse(e.evaluation_time));
  evals.truncate(10);
  evals
}

#[component]
async fn summary(project: &Project, mutable: bool) -> Result<impl View> {
  let repository = repository_page_url(&project.repository_url);

  Ok(view! {
    <h1>(project.name.clone())</h1>
    if let Some(description) = project.description.clone() {
      <p>(description)</p>
    }
    <p>
      <strong>"Repository:"</strong>
      " "
      match repository {
        Some(href) => <a href=(href.to_string())>(project.repository_url.clone())</a>,
        None => (project.repository_url.clone()),
      }
    </p>
    <p>
      <strong>"Created:"</strong>
      " "
      local_time(at: project.created_at, zone: false)
    </p>
    if project.managed_declaratively && !mutable {
      <p class="flash-message">
        "Managed by declarative configuration. Runtime changes are disabled."
      </p>
    }
  })
}

#[component]
async fn actions(
  project_id: Uuid,
  mutable: bool,
  csrf_token: &str,
  rejected: Option<Rejected>,
) -> Result<impl View> {
  Ok(view! {
    <div class="page-actions">
      if mutable && rejected.is_none() {
        <button
          type="button"
          class="btn btn-small btn-secondary"
          onclick="document.getElementById('add-jobset-dialog').showModal()"
        >
          "Add Jobset"
        </button>
      }
      <a
        class="btn btn-small btn-secondary"
        href=(format!("/project/{project_id}/notifications"))
      >
        "Notifications"
      </a>
      if mutable {
        <form
          class="inline"
          method="POST"
          action=(format!("/project/{project_id}/delete"))
        >
          <input type="hidden" name="csrf_token" value=(csrf_token)>
          confirm_button(
            prompt: "Delete this project and all its data?",
            class: "btn btn-small btn-danger",
            label: "Delete Project",
          )
        </form>
      }
    </div>
    if mutable {
      if let Some(Rejected { form, message }) = rejected {
        <section class="panel">
          <div class="panel-header">
            <h2>"Add Jobset"</h2>
          </div>
          <div class="flash-message flash-error">(message)</div>
          add_jobset_form(project_id: project_id, csrf_token: csrf_token, draft: Some(&form))
          <div class="dialog-footer">
            <a class="btn btn-secondary" href=(format!("/project/{project_id}"))>"Cancel"</a>
            <button type="submit" class="btn" form="create-jobset-form">"Add Jobset"</button>
          </div>
        </section>
      } else {
        <dialog id="add-jobset-dialog">
          <div class="dialog-header">
            <h2>"Add Jobset"</h2>
            <button
              type="button"
              class="btn-ghost dialog-close"
              aria-label="Close"
              onclick="document.getElementById('add-jobset-dialog').close()"
            >
              "×"
            </button>
          </div>
          <div class="dialog-body">
            add_jobset_form(project_id: project_id, csrf_token: csrf_token, draft: None)
          </div>
          <div class="dialog-footer">
            <button
              type="button"
              class="btn btn-secondary"
              onclick="document.getElementById('add-jobset-dialog').close()"
            >
              "Cancel"
            </button>
            <button type="submit" class="btn" form="create-jobset-form">"Add Jobset"</button>
          </div>
        </dialog>
      }
    }
  })
}

#[component]
async fn add_jobset_form(
  project_id: Uuid,
  csrf_token: &str,
  draft: Option<&JobsetForm>,
) -> Result<impl View> {
  let name = draft.map_or("", |draft| draft.name.as_str());
  let expression = draft.map_or(".", |draft| draft.nix_expression.as_str());
  let flake = draft.is_none_or(|draft| draft.flake_mode.is_some());
  let interval = draft
    .is_some_and(|draft| draft.trigger_mode == JobsetTriggerMode::Interval);
  let latest = draft.is_some_and(|draft| draft.only_build_latest.is_some());
  let path_filters = draft.map_or("", |draft| draft.path_filters.as_str());

  Ok(view! {
    <form
      id="create-jobset-form"
      method="POST"
      action=(format!("/project/{project_id}/jobsets"))
    >
      <input type="hidden" name="csrf_token" value=(csrf_token)>
      <div class="form-group">
        <label for="js-name">"Name"</label>
        <input type="text" id="js-name" name="name" value=(name) required=(true)>
      </div>
      <div class="form-group">
        <label for="js-expr">"Nix Expression"</label>
        <input type="text" id="js-expr" name="nix_expression" value=(expression) required=(true)>
      </div>
      <div class="form-group">
        <label>
          <input type="checkbox" id="js-flake" name="flake_mode" checked=(flake)>
          " Flake mode"
        </label>
      </div>
      <div class="form-group">
        <label for="js-trigger-mode">"Trigger Mode"</label>
        <select id="js-trigger-mode" name="trigger_mode">
          <option value="source_change" selected=(!interval)>"Source change"</option>
          <option value="interval" selected=(interval)>"Interval rebuild"</option>
        </select>
      </div>
      <div class="form-group">
        <label>
          <input type="checkbox" id="js-only-build-latest" name="only_build_latest" checked=(latest)>
          " Only build latest revision"
        </label>
        <small>
          "Cancel older automated work in the same branch, change request, or tag stream."
        </small>
      </div>
      <div class="form-group">
        <label for="js-path-filters">"Path filters"</label>
        <textarea
          id="js-path-filters"
          name="path_filters"
          rows="3"
          placeholder="packages/hardened-kernel\n**/*.nix"
        >(path_filters)</textarea>
        <small>"One Git pathspec per line. Evaluate when any matching path changes."</small>
      </div>
    </form>
  })
}

#[component]
async fn jobsets_panel(
  jobsets: Vec<Jobset>,
  manage: bool,
  csrf_token: &str,
) -> Result<impl View> {
  Ok(view! {
    <section class="panel">
      <div class="panel-header">
        <h2 id="jobsets">"Jobsets"</h2>
      </div>
      if jobsets.is_empty() {
        <div class="empty compact-empty">
          <div class="empty-title">"No jobsets configured"</div>
          if manage {
            <div class="empty-hint">"Add a jobset above to start evaluating this project."</div>
          }
        </div>
      } else {
        <div class="table-wrap compact-table-wrap">
          <table>
            <thead>
              <tr>
                <th>"Name"</th>
                <th>"Expression"</th>
                <th>"Flake"</th>
                <th>"Enabled"</th>
                <th>"Trigger"</th>
                <th>"Interval"</th>
                if manage {
                  <th>"Actions"</th>
                }
              </tr>
            </thead>
            <tbody>
              for jobset in jobsets {
                jobset_row(jobset: jobset, manage: manage, csrf_token: csrf_token)
              }
            </tbody>
          </table>
        </div>
      }
    </section>
  })
}

#[component]
async fn jobset_row(
  jobset: Jobset,
  manage: bool,
  csrf_token: &str,
) -> Result<impl View> {
  let yes_no = |value: bool| if value { "Yes" } else { "No" };
  let trigger = match jobset.trigger_mode {
    JobsetTriggerMode::SourceChange => "Source change",
    JobsetTriggerMode::Interval => "Interval rebuild",
  };

  Ok(view! {
    <tr>
      <td><a href=(format!("/jobset/{}", jobset.id))>(jobset.name)</a></td>
      <td><code>(jobset.nix_expression)</code></td>
      <td>(yes_no(jobset.flake_mode))</td>
      <td>(yes_no(jobset.enabled))</td>
      <td>(trigger)</td>
      <td>(jobset.check_interval) "s"</td>
      if manage {
        <td>
          <form
            class="inline"
            method="POST"
            action=(format!("/jobset/{}/delete", jobset.id))
          >
            <input type="hidden" name="csrf_token" value=(csrf_token)>
            confirm_button(
              prompt: "Delete this jobset and all of its evaluations/builds?",
              class: "btn btn-danger btn-small",
              label: "Delete",
            )
          </form>
        </td>
      }
    </tr>
  })
}

#[component]
async fn evaluations_panel(
  evals: Vec<Evaluation>,
  project_id: Uuid,
  admin: bool,
  csrf_token: &str,
) -> Result<impl View> {
  Ok(view! {
    <section class="panel" style="margin-top: 14px">
      <div class="panel-header">
        <h2 id="evaluations">"Recent Evaluations"</h2>
      </div>
      if evals.is_empty() {
        <div class="empty compact-empty">
          <div class="empty-title">"No evaluations yet"</div>
          <div class="empty-hint">"Evaluations appear once a jobset runs."</div>
        </div>
      } else {
        <div class="table-wrap compact-table-wrap">
          <table>
            <thead>
              <tr>
                <th>"Commit"</th>
                <th>"Status"</th>
                <th>"Time"</th>
                if admin {
                  <th>"Actions"</th>
                }
              </tr>
            </thead>
            <tbody>
              #[key(eval.id.as_u128())]
              for eval in evals {
                evaluation_row(
                  view: eval_view(&eval),
                  at: eval.evaluation_time,
                  project_id: project_id,
                  admin: admin,
                  csrf_token: csrf_token,
                )
              }
            </tbody>
          </table>
        </div>
      }
    </section>
  })
}

#[component]
async fn evaluation_row(
  view: EvalView,
  at: jiff::Timestamp,
  project_id: Uuid,
  admin: bool,
  csrf_token: &str,
) -> Result<impl View> {
  let hidden = view.hidden;

  Ok(view! {
    <tr>
      <td><a href=(format!("/evaluation/{}", view.id))>(view.commit_short)</a></td>
      <td>
        <span class=(format!("badge badge-{}", view.status_class))>(view.status_text)</span>
        if hidden {
          <span class="badge badge-skipped">"Hidden"</span>
        }
      </td>
      <td>local_time(at: at, zone: false)</td>
      if admin {
        <td>
          <form
            class="inline"
            method="POST"
            action=(format!("/evaluation/{}/visibility", view.id))
          >
            <input type="hidden" name="csrf_token" value=(csrf_token)>
            <input type="hidden" name="return_to" value=(format!("/project/{project_id}"))>
            <input type="hidden" name="hidden" value=(if hidden { "false" } else { "true" })>
            <button class="btn btn-small btn-secondary" type="submit">
              (if hidden { "Unhide" } else { "Hide" })
            </button>
          </form>
        </td>
      }
    </tr>
  })
}

#[cfg(test)]
mod tests {
  use circus_common::models::BinaryCacheUpstreams;
  use jiff::Timestamp;
  use topcoat::router::{Body, Router, request::Request, to_bytes};

  use super::*;

  #[page("/__test/declarative-project")]
  async fn declarative_project() -> Result<impl View> {
    let project = Project {
      id:                     Uuid::nil(),
      name:                   "declarative".into(),
      description:            None,
      repository_url:         "https://example.com/project".into(),
      cache_enabled:          false,
      cache_url:              None,
      cache_upstreams:        BinaryCacheUpstreams::default(),
      managed_declaratively:  true,
      allow_runtime_mutation: Some(false),
      created_at:             Timestamp::now(),
      updated_at:             Timestamp::now(),
    };

    Ok(view! {
      summary(project: &project, mutable: false)
      actions(project_id: project.id, mutable: false, csrf_token: "csrf", rejected: None)
      jobsets_panel(jobsets: Vec::new(), manage: false, csrf_token: "csrf")
    })
  }

  #[tokio::test]
  async fn declarative_project_hides_mutation_controls() {
    let router = Router::builder().page(declarative_project).build();
    let request = Request::builder()
      .uri("/__test/declarative-project")
      .body(Body::empty())
      .expect("valid request");
    let body = to_bytes(router.handle(request).await.into_body(), usize::MAX)
      .await
      .expect("readable body");
    let html = String::from_utf8(body.to_vec()).expect("utf-8 body");

    assert!(html.contains("Managed by declarative configuration"));
    assert!(html.contains(">Notifications</a>"));
    assert!(!html.contains("Add Jobset"));
    assert!(!html.contains("Delete Project"));
  }
}
