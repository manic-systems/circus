//! The project list and the new project wizard.

use circus_nix::{FlakeProbeResult, SuggestedJobset};
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{page, query_params},
  view::{View, component, emit, live, view},
};

use super::super::{
  components::local_time,
  layout::{admin_viewer, document, viewer},
  shared::{DashboardPage, Pagination},
};
use crate::{routes::projects::probe, state::AppState};

#[query_params(error = bad_request)]
struct ProjectsQuery {
  limit:  Option<i64>,
  offset: Option<i64>,
  error:  Option<String>,
}

/// Why a dashboard project form was turned away, carried as `?error=`.
#[derive(Clone, Copy)]
pub(in crate::routes::dashboard) enum CreateError {
  Invalid,
  Exists,
  Failed,
}

impl CreateError {
  pub(in crate::routes::dashboard) const fn code(self) -> &'static str {
    match self {
      Self::Invalid => "invalid",
      Self::Exists => "exists",
      Self::Failed => "failed",
    }
  }

  fn from_code(code: &str) -> Option<Self> {
    match code {
      "invalid" => Some(Self::Invalid),
      "exists" => Some(Self::Exists),
      "failed" => Some(Self::Failed),
      _ => None,
    }
  }

  const fn message(self) -> &'static str {
    match self {
      Self::Invalid => {
        "The project name or repository URL is not valid. Names use letters, \
         digits, dashes, dots and underscores."
      },
      Self::Exists => "A project with this name already exists.",
      Self::Failed => "The project could not be created. Please try again.",
    }
  }
}

impl From<&circus_common::CiError> for CreateError {
  fn from(error: &circus_common::CiError) -> Self {
    match error {
      circus_common::CiError::Validation(_) => Self::Invalid,
      circus_common::CiError::Conflict(_) => Self::Exists,
      _ => Self::Failed,
    }
  }
}

#[page("/projects")]
async fn projects_page(cx: &Cx) -> Result<impl View> {
  let viewer = viewer(cx, DashboardPage::Projects).await?;
  let state = app_context::<AppState>(cx);
  let query = query_params::<ProjectsQuery>(cx)?;
  let limit = query.limit.unwrap_or(50).clamp(1, 200);
  let offset = query.offset.unwrap_or(0).max(0);
  let error = query.error.as_deref().and_then(CreateError::from_code);
  let projects =
    circus_common::repo::projects::list(&state.pool, limit, offset)
      .await
      .unwrap_or_default();
  let total = circus_common::repo::projects::count(&state.pool)
    .await
    .unwrap_or(0);
  let pagination = Pagination::new(total, offset, limit);
  let page_href =
    move |offset: i64| format!("/projects?offset={offset}&limit={limit}");

  Ok(view! {
    document(title: "Projects", viewer: &viewer,
      <nav class="breadcrumbs">
        <a href="/">"Home"</a>
        <span class="sep">"/"</span>
        <span class="current">"Projects"</span>
      </nav>
      <h1>"Projects"</h1>

      if let Some(error) = error {
        <div class="flash-message flash-error">(error.message())</div>
      }

      if viewer.is_admin {
        <div class="page-actions">
          <a class="btn" href="/projects/new">"Probe repository"</a>
          <button
            type="button"
            class="btn btn-secondary"
            onclick="document.getElementById('quick-create-dialog').showModal()"
          >
            "Quick create"
          </button>
        </div>

        <dialog id="quick-create-dialog">
          <div class="dialog-header">
            <h2>"Quick create"</h2>
            <button
              type="button"
              class="btn-ghost dialog-close"
              onclick="document.getElementById('quick-create-dialog').close()"
              aria-label="Close"
            >
              "×"
            </button>
          </div>
          <div class="dialog-body">
            <form id="create-project-form" method="POST" action="/projects">
              <input type="hidden" name="csrf_token" value=(viewer.csrf_token.as_str())>
              <div class="form-group">
                <label for="project-name">"Name"</label>
                <input type="text" id="project-name" name="name" required=(true)>
              </div>
              <div class="form-group">
                <label for="project-repo">"Repository URL"</label>
                <input type="url" id="project-repo" name="repository_url" required=(true)>
              </div>
              <div class="form-group">
                <label for="project-desc">"Description"</label>
                <textarea id="project-desc" name="description" rows="3"></textarea>
              </div>
            </form>
          </div>
          <div class="dialog-footer">
            <button
              type="button"
              class="btn btn-secondary"
              onclick="document.getElementById('quick-create-dialog').close()"
            >
              "Cancel"
            </button>
            <button type="submit" class="btn" form="create-project-form">"Create Project"</button>
          </div>
        </dialog>
      }

      if projects.is_empty() {
        <div class="empty">
          <div class="empty-title">"No projects yet"</div>
          if viewer.is_admin {
            <div class="empty-hint">"Create a project using the buttons above to get started."</div>
          } else {
            <div class="empty-hint">"Projects will appear here once an administrator creates them."</div>
          }
        </div>
      } else {
        <div class="table-wrap">
          <table class="data-table">
            <thead>
              <tr>
                <th>"Name"</th>
                <th>"Description"</th>
                <th>"Repository"</th>
                <th>"Created"</th>
                <th>"Updated"</th>
              </tr>
            </thead>
            <tbody>
              #[key(*project.id.as_bytes())]
              for project in projects {
                <tr>
                  <td class="truncate" title=(project.name.as_str())>
                    <a href=(format!("/project/{}", project.id))>(project.name.as_str())</a>
                  </td>
                  <td class="truncate text-muted" title=(project.description.as_deref().unwrap_or_default())>
                    (project.description.as_deref().unwrap_or("-"))
                  </td>
                  <td class="mono truncate" title=(project.repository_url.as_str())>
                    (project.repository_url.as_str())
                  </td>
                  <td class="nowrap">local_time(at: project.created_at)</td>
                  <td class="nowrap">local_time(at: project.updated_at)</td>
                </tr>
              }
            </tbody>
          </table>
        </div>
        if pagination.total_pages > 1 {
          <nav class="pagination">
            if pagination.has_prev {
              <a href=(page_href(pagination.prev_offset)) class="btn btn-small btn-secondary">"« Previous"</a>
            }
            <span class="text-muted">"Page " (pagination.page) " of " (pagination.total_pages)</span>
            if pagination.has_next {
              <a href=(page_href(pagination.next_offset)) class="btn btn-small btn-secondary">"Next »"</a>
            }
          </nav>
        }
      }
    )
  })
}

#[query_params(error = bad_request)]
struct SetupQuery {
  url:        Option<String>,
  error:      Option<String>,
  csrf_token: Option<String>,
}

/// Step 1 asks for a repository, then the probe streams in the jobset choices
/// and project details as one form posted to `/projects/setup`.
#[page("/projects/new")]
async fn project_setup_page(cx: &Cx) -> Result<impl View> {
  let viewer = admin_viewer(cx).await?;
  let query = query_params::<SetupQuery>(cx)?;
  let url = query
    .url
    .as_deref()
    .map(str::trim)
    .filter(|url| !url.is_empty())
    .map(str::to_owned);
  let error = query.error.as_deref().and_then(CreateError::from_code);
  // Any link could otherwise make the server evaluate a flake of its choosing.
  let probe_url = url.clone().filter(|_| {
    query
      .csrf_token
      .as_deref()
      .is_some_and(|token| viewer.check_csrf(token).is_ok())
  });

  Ok(view! {
    document(title: "New Project", viewer: &viewer,
      <nav class="breadcrumbs">
        <a href="/">"Home"</a>
        <span class="sep">"/"</span>
        <a href="/projects">"Projects"</a>
        <span class="sep">"/"</span>
        <span class="current">"New Project"</span>
      </nav>
      <div id="wizard">
        <h1>"New Project Setup"</h1>
        <div class="wizard-step">
          <h2>"Step 1: Repository URL"</h2>
          <div class="form-card form-card-wide">
            <form method="get" action="/projects/new">
              <input type="hidden" name="csrf_token" value=(viewer.csrf_token.as_str())>
              <div class="form-group">
                <label for="probe-url">"Flake repository URL"</label>
                <input
                  type="url"
                  id="probe-url"
                  name="url"
                  placeholder="https://github.com/user/repo"
                  value=(url.as_deref().unwrap_or_default())
                  required=(true)
                >
              </div>
              <button type="submit" class="btn">"Probe Repository"</button>
            </form>
            if let Some(error) = error {
              <div class="form-status">
                <div class="flash-message flash-error">(error.message())</div>
              </div>
            }
          </div>
        </div>
        if let Some(url) = probe_url {
          probe_results(url: url, csrf_token: viewer.csrf_token.clone())
        }
      </div>
    )
  })
}

#[component]
async fn probe_results(
  cx: &Cx,
  url: String,
  csrf_token: String,
) -> Result<impl View> {
  let state = app_context::<AppState>(cx).clone();

  Ok(view! {
    (live! {
      emit! {
        <div class="form-status">
          <span class="spinner"></span>
          <span class="text-muted">"Probing repository… This may take a moment."</span>
        </div>
      }?;

      match probe(&state, &url, None).await {
        Ok(result) if result.is_flake => {
          emit! { setup_form(url: url, csrf_token: csrf_token, result: result) }
        },
        Ok(result) => {
          let message = result.error.unwrap_or_else(|| "Not a flake repository".to_owned());
          emit! { probe_error(message: message) }
        },
        Err(error) => emit! { probe_error(message: error.to_string()) },
      }
    })
  })
}

#[component]
async fn probe_error(message: String) -> Result<impl View> {
  Ok(view! {
    <div class="form-status">
      <div class="flash-message flash-error">(message)</div>
    </div>
  })
}

fn default_name(url: &str) -> String {
  url
    .trim_end_matches('/')
    .trim_end_matches(".git")
    .rsplit('/')
    .next()
    .unwrap_or_default()
    .to_owned()
}

#[component]
async fn setup_form(
  url: String,
  csrf_token: String,
  result: FlakeProbeResult,
) -> Result<impl View> {
  let name = default_name(&url);
  let description = result.metadata.description.clone().unwrap_or_default();
  let outputs = result.outputs;
  let suggestions = result.suggested_jobsets;

  Ok(view! {
    <form method="POST" action="/projects/setup">
      <input type="hidden" name="csrf_token" value=(csrf_token)>
      <input type="hidden" name="repository_url" value=(url)>

      <div class="wizard-step">
        <h2>"Step 2: Select Jobsets"</h2>
        <p class="wizard-hint">"Select which outputs to build. You can customise the Nix expression for each."</p>
        if suggestions.is_empty() {
          <div class="empty">"No buildable outputs detected."</div>
        } else {
          <div class="table-wrap">
            <table class="probe-outputs-table">
              <thead>
                <tr>
                  <th></th>
                  <th>"Name"</th>
                  <th>"Expression"</th>
                  <th>"Systems"</th>
                  <th>"Description"</th>
                  <th>"Priority"</th>
                </tr>
              </thead>
              <tbody>
                for (index, jobset) in suggestions.into_iter().enumerate() {
                  suggestion_row(index: index, jobset: jobset)
                }
              </tbody>
            </table>
          </div>
        }
        if !outputs.is_empty() {
          <details class="outputs-detail">
            <summary>"All detected outputs (" (outputs.len()) ")"</summary>
            <ul class="outputs-list">
              for output in outputs {
                <li>
                  <span class="output-path">(output.path)</span>
                  <span class="output-meta">
                    (output.output_type)
                    if !output.systems.is_empty() {
                      " · " (output.systems.join(", "))
                    }
                  </span>
                </li>
              }
            </ul>
          </details>
        }
      </div>

      <div class="wizard-step">
        <h2>"Step 3: Project Details"</h2>
        <div class="form-card form-card-wide">
          <div class="form-group">
            <label for="project-name">"Project Name"</label>
            <input type="text" id="project-name" name="name" value=(name) required=(true)>
          </div>
          <div class="form-group">
            <label for="project-desc">"Description"</label>
            <textarea id="project-desc" name="description">(description)</textarea>
          </div>
          <div class="wizard-actions">
            <button type="submit" class="btn">"Create Project"</button>
            <a class="btn btn-secondary" href="/projects/new">"Start over"</a>
          </div>
        </div>
      </div>
    </form>
  })
}

#[component]
async fn suggestion_row(
  index: usize,
  jobset: SuggestedJobset,
) -> Result<impl View> {
  let systems = jobset.systems.len();

  Ok(view! {
    <tr>
      <td>
        <input
          type="checkbox"
          id=(format!("setup-jobset-check-{index}"))
          name="jobset"
          value=(index.to_string())
          aria-label=(format!("Include {}", jobset.name))
          checked=(jobset.priority >= 6)
        >
        <input type="hidden" name=(format!("name_{index}")) value=(jobset.name.as_str())>
        for system in &jobset.systems {
          <input type="hidden" name=(format!("detected_{index}")) value=(system.as_str())>
        }
      </td>
      <td>(jobset.name.as_str())</td>
      <td>
        <input
          type="text"
          id=(format!("setup-jobset-expr-{index}"))
          name=(format!("expr_{index}"))
          class="js-expr inline-input"
          aria-label=(format!("Nix expression for {}", jobset.name))
          value=(jobset.nix_expression.as_str())
        >
      </td>
      <td>
        if systems == 0 {
          <span class="text-muted">"—"</span>
        } else {
          <details class="sys-picker">
            <summary class="btn btn-small btn-secondary">
              (if systems == 1 { "1 system".to_owned() } else { format!("{systems} systems") })
            </summary>
            <div class="sys-drawer-list">
              for system in jobset.systems {
                <label class="sys-item">
                  <input type="checkbox" name=(format!("systems_{index}")) value=(system.as_str()) checked=(true)>
                  " " (system.as_str())
                </label>
              }
            </div>
          </details>
        }
      </td>
      <td class="text-muted text-sm">(jobset.description)</td>
      <td class="text-center">(jobset.priority)</td>
    </tr>
  })
}
