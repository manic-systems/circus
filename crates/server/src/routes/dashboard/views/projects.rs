//! The project list and the new project wizard.

use circus_nix::{FlakeProbeResult, SuggestedJobset};
use serde::Deserialize;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Form,
    error::{bad_request, forbidden, see_other},
    page,
    query_params,
  },
  view::{View, component, emit, live, view},
};

use super::super::{
  components::local_time,
  layout::{admin_viewer, document, viewer},
  shared::{DashboardContext, DashboardPage, Pagination},
};
use crate::{
  routes::projects::{SetupJobsetInput, SetupProjectRequest, probe, setup},
  state::AppState,
};

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
                  <td class="nowrap">local_time(at: project.created_at, zone: false)</td>
                  <td class="nowrap">local_time(at: project.updated_at, zone: false)</td>
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
  url: Option<String>,
}

#[derive(Deserialize)]
struct ProbeForm {
  csrf_token: String,
  url:        String,
}

#[page("/projects/new")]
async fn project_setup_page(cx: &Cx) -> Result<impl View> {
  let viewer = admin_viewer(cx).await?;
  let url = query_params::<SetupQuery>(cx)?.url.clone();

  Ok(view! { setup_view(viewer: viewer, url: url, error: None, probe: false) })
}

/// Probes the posted repository, streaming in the jobset choices and project
/// details as one form posted to `/projects/setup`. A POST, since any link
/// could otherwise make the server evaluate a flake of its choosing.
#[page(POST "/projects/new")]
async fn project_probe(
  cx: &Cx,
  Form(input): Form<ProbeForm>,
) -> Result<impl View> {
  let viewer = admin_viewer(cx).await?;
  viewer
    .check_csrf(&input.csrf_token)
    .map_err(|_| forbidden())?;
  let url = Some(input.url.trim().to_owned()).filter(|url| !url.is_empty());
  let probe = url.is_some();

  Ok(view! { setup_view(viewer: viewer, url: url, error: None, probe: probe) })
}

/// Creates the project from the wizard, answering a failure with step 1 and
/// the reason.
#[page(POST "/projects/setup")]
async fn project_setup(
  cx: &Cx,
  Form(fields): Form<Vec<(String, String)>>,
) -> Result<impl View> {
  let viewer = admin_viewer(cx).await?;
  let state = app_context::<AppState>(cx);
  let csrf_token = fields
    .iter()
    .find(|(name, _)| name == "csrf_token")
    .map_or("", |(_, value)| value.as_str());
  viewer.check_csrf(csrf_token).map_err(|_| forbidden())?;

  let request = setup_request(&fields)
    .ok_or_else(|| bad_request("incomplete project setup form"))?;
  let url = request.repository_url.clone();

  match setup(state, request).await {
    Ok((project, _)) => {
      Err(see_other(format!("/project/{}", project.id)).into())
    },
    Err(error) => {
      tracing::warn!("Failed to set up project: {error}");
      let (url, error) = (Some(url), Some(error.to_string()));
      Ok(
        view! { setup_view(viewer: viewer, url: url, error: error, probe: false) },
      )
    },
  }
}

/// Builds the setup request from the wizard form, whose jobset fields are
/// suffixed with the suggestion index and repeat for each chosen system.
fn setup_request(fields: &[(String, String)]) -> Option<SetupProjectRequest> {
  let value = |key: &str| {
    fields
      .iter()
      .find(|(name, _)| name == key)
      .map(|(_, value)| value.trim().to_owned())
  };
  let values = |key: &str| {
    fields
      .iter()
      .filter(|(name, _)| name == key)
      .map(|(_, value)| value.clone())
      .collect::<Vec<_>>()
  };

  let mut jobsets = Vec::new();
  for index in values("jobset") {
    let index: usize = index.parse().ok()?;
    let detected = values(&format!("detected_{index}"));
    let picked = values(&format!("systems_{index}"));
    jobsets.push(SetupJobsetInput {
      name:           value(&format!("name_{index}"))?,
      nix_expression: value(&format!("expr_{index}"))?,
      systems:        (picked.len() != detected.len()).then_some(picked),
    });
  }

  let description = value("description").filter(|text| !text.is_empty());
  Some(SetupProjectRequest {
    repository_url: value("repository_url")?,
    name: value("name")?,
    description,
    jobsets,
  })
}

#[component]
async fn setup_view(
  viewer: DashboardContext,
  url: Option<String>,
  error: Option<String>,
  probe: bool,
) -> Result<impl View> {
  let probe_url = url.clone().filter(|_| probe);

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
            <form method="POST" action="/projects/new">
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
                <div class="flash-message flash-error">(error)</div>
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
