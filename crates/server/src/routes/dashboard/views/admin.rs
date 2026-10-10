//! The admin overview, and the form posts that answer with it.

use axum::extract::Query;
use circus_common::roles::GlobalRole;
use serde::Deserialize;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Form,
    error::{bad_request, forbidden},
    header::{CACHE_CONTROL, HeaderValue},
    page,
    request,
    response::response_headers,
  },
  view::{View, component, view},
};

use super::super::{
  admin::{AdminOverview, AdminParams, load_overview},
  components::{confirm_button, copy_button, local_time},
  layout::{admin_session, admin_viewer, document},
  shared::DashboardContext,
};
use crate::{
  routes::{admin::write_config_file, auth::generate_api_key},
  state::AppState,
};

const KEY_ROLES: [GlobalRole; 7] = [
  GlobalRole::Admin,
  GlobalRole::ReadOnly,
  GlobalRole::CreateProjects,
  GlobalRole::EvalJobset,
  GlobalRole::CancelBuild,
  GlobalRole::RestartJobs,
  GlobalRole::BumpToFront,
];

struct Flash {
  success: bool,
  message: String,
}

impl Flash {
  fn success(message: impl Into<String>) -> Self {
    Self {
      success: true,
      message: message.into(),
    }
  }

  fn error(message: impl Into<String>) -> Self {
    Self {
      success: false,
      message: message.into(),
    }
  }
}

/// Result messages, each shown in the section its form lives in.
#[derive(Default)]
struct Notices {
  gc_requested: bool,
  maintenance:  Option<Flash>,
  pinned:       Option<Flash>,
  keys:         Option<Flash>,
  new_key:      Option<String>,
  config:       Option<Flash>,
  tasks:        Option<Flash>,
}

impl Notices {
  fn from_params(params: &AdminParams) -> Self {
    let maintenance =
      match (params.gc.as_deref(), params.failed_paths.as_deref()) {
        (Some("error"), _) => {
          Some(Flash::error("Failed to request a GC cycle"))
        },
        (_, Some("cleared")) => {
          Some(Flash::success(format!(
            "Cleared {} entries and requeued {} cached failures.",
            params.deleted.unwrap_or(0),
            params.restarted.unwrap_or(0),
          )))
        },
        (_, Some("error")) => {
          Some(Flash::error("Failed to clear failed paths cache"))
        },
        _ => None,
      };
    let pinned = match params.unpinned.as_deref() {
      Some("ok") => Some(Flash::success("Build unpinned.")),
      Some("error") => Some(Flash::error("Failed to unpin build")),
      _ => None,
    };
    let keys = match params.key.as_deref() {
      Some("deleted") => Some(Flash::success("API key deleted.")),
      Some("error") => Some(Flash::error("Failed to delete key")),
      _ => None,
    };
    let tasks = match params.task.as_deref() {
      Some("retried") => Some(Flash::success("Notification task requeued.")),
      Some("error") => Some(Flash::error("Failed to retry notification task")),
      _ => None,
    };

    Self {
      gc_requested: params.gc.as_deref() == Some("requested"),
      maintenance,
      pinned,
      keys,
      new_key: None,
      config: None,
      tasks,
    }
  }
}

#[page("/admin")]
async fn admin_page(cx: &Cx) -> Result<impl View> {
  let viewer = admin_viewer(cx).await?;
  let state = app_context::<AppState>(cx);
  let Query(params) = Query::<AdminParams>::try_from_uri(request::uri(cx))
    .map_err(|_| bad_request("invalid admin query"))?;
  let overview = load_overview(state, &params).await;
  let notices = Notices::from_params(&params);

  Ok(
    view! { admin_view(viewer: &viewer, overview: overview, notices: notices) },
  )
}

#[derive(Deserialize)]
struct CreateKeyForm {
  csrf_token: String,
  name:       String,
  role:       GlobalRole,
}

/// Creates an API key and answers with the admin page, the only place its
/// secret is ever shown.
#[page(POST "/admin/api-keys")]
async fn api_key_create(
  cx: &Cx,
  Form(input): Form<CreateKeyForm>,
) -> Result<impl View> {
  let (viewer, extensions) = admin_session(cx).await?;
  viewer
    .check_csrf(&input.csrf_token)
    .map_err(|_| forbidden())?;
  response_headers(cx)
    .append(CACHE_CONTROL, HeaderValue::from_static("no-store"));
  let state = app_context::<AppState>(cx);
  let mut notices = Notices::default();

  let created = match generate_api_key() {
    Ok((key, hash)) => {
      circus_common::repo::api_keys::create(
        &state.pool,
        &input.name,
        &hash,
        input.role,
      )
      .await
      .map(|api_key| (key, api_key))
    },
    Err(error) => Err(error),
  };

  match created {
    Ok((key, api_key)) => {
      crate::audit::record_action(
        &state.pool,
        &extensions,
        "API_KEY_CREATE",
        Some("api_key"),
        Some(&api_key.id.to_string()),
        serde_json::json!({ "name": api_key.name, "role": api_key.role }),
      )
      .await;
      notices.new_key = Some(key);
    },
    Err(error) => {
      tracing::error!("failed to create API key: {error}");
      notices.keys = Some(Flash::error(error.to_string()));
    },
  }

  let overview = load_overview(state, &AdminParams::default()).await;
  Ok(
    view! { admin_view(viewer: &viewer, overview: overview, notices: notices) },
  )
}

#[derive(Deserialize)]
struct ConfigForm {
  csrf_token: String,
  contents:   String,
}

/// Writes the config file and answers with the admin page, keeping the
/// submitted text in the editor when it is rejected.
#[page(POST "/admin/config")]
async fn config_save(
  cx: &Cx,
  Form(input): Form<ConfigForm>,
) -> Result<impl View> {
  let (viewer, extensions) = admin_session(cx).await?;
  viewer
    .check_csrf(&input.csrf_token)
    .map_err(|_| forbidden())?;
  response_headers(cx)
    .append(CACHE_CONTROL, HeaderValue::from_static("no-store"));
  let state = app_context::<AppState>(cx);
  let mut notices = Notices::default();

  let rejected = match write_config_file(state, &input.contents).await {
    Ok((path, rendered)) => {
      crate::audit::record_action(
        &state.pool,
        &extensions,
        "CONFIG_UPDATE",
        Some("config"),
        Some(&path.display().to_string()),
        serde_json::json!({ "bytes": rendered.len() }),
      )
      .await;
      notices.config = Some(Flash::success(
        "Configuration saved. Restart daemons to apply runtime changes.",
      ));
      None
    },
    Err(error) => {
      notices.config = Some(Flash::error(error.to_string()));
      Some(input.contents)
    },
  };

  let mut overview = load_overview(state, &AdminParams::default()).await;

  if let Some(contents) = rejected {
    overview.config_contents = contents;
  }

  Ok(
    view! { admin_view(viewer: &viewer, overview: overview, notices: notices) },
  )
}

#[component]
async fn flash(notice: Option<Flash>) -> Result<impl View> {
  Ok(view! {
    if let Some(notice) = notice {
      <div class=(if notice.success { "flash-message flash-success" } else { "flash-message flash-error" })>
        (notice.message)
      </div>
    }
  })
}

#[component]
async fn empty(title: &str, hint: &str) -> Result<impl View> {
  Ok(view! {
    <div class="empty compact-empty">
      <div class="empty-title">(title)</div>
      <div class="empty-hint">(hint)</div>
    </div>
  })
}

#[component]
async fn csrf(token: &str) -> Result<impl View> {
  Ok(view! { <input type="hidden" name="csrf_token" value=(token)> })
}

#[component]
async fn admin_view(
  viewer: &DashboardContext,
  overview: AdminOverview,
  notices: Notices,
) -> Result<impl View> {
  let token = viewer.csrf_token.as_str();
  let status = &overview.status;
  let stats = [
    (status.projects_count, "Projects"),
    (status.jobsets_count, "Jobsets"),
    (status.evaluations_count, "Evaluations"),
    (status.builds_pending, "Pending"),
    (status.builds_running, "Running"),
    (status.builds_completed, "Completed"),
    (status.builds_failed, "Failed"),
    (status.channels_count, "Channels"),
  ];

  Ok(view! {
    document(title: "Admin", viewer: viewer,
      <h1>"Administration"</h1>

      <section class="panel">
        <div class="panel-header"><h2>"System Status"</h2></div>
        <div class="panel-body">
          <div class="stats-grid">
            for (value, label) in stats {
              <div class="stat-card">
                <div class="stat-value">(value)</div>
                <div class="stat-label">(label)</div>
              </div>
            }
          </div>
        </div>
      </section>

      <section class="panel">
        <div class="panel-header"><h2>"Store Maintenance"</h2></div>
        <div class="panel-body">
          if notices.gc_requested {
            <div class="flash-message flash-success">
              "GC cycle requested. The queue runner cleans up aged roots and runs "
              <code>"nix-collect-garbage"</code>
              "; results land in its logs."
            </div>
          }
          flash(notice: notices.maintenance)
          if overview.gc_enabled {
            <form method="POST" action="/admin/store-gc" class="filter-form">
              csrf(token: token)
              confirm_button(
                prompt: "Run a GC cycle now? Unpinned roots older than the configured maximum age are removed and unreferenced store paths are deleted.",
                class: "btn btn-danger",
                label: "Run GC now",
              )
            </form>
            <p class="empty-hint">
              "Ages out unpinned GC roots and runs "
              <code>"nix-collect-garbage"</code>
              " on the queue runner host. Pinned build outputs are kept."
            </p>
          } else {
            <p class="empty-hint">
              "GC is disabled in the configuration ("
              <code>"[gc] enabled = false"</code>
              "), so a manual cycle cannot be requested."
            </p>
          }
          <form method="POST" action="/admin/failed-paths/clear" class="filter-form">
            csrf(token: token)
            confirm_button(
              prompt: "Clear all failed-path cache records and requeue matching cached failures? This does not delete Nix store paths, logs, or build history.",
              class: "btn btn-danger",
              label: "Clear failed paths cache and requeue cached failures",
            )
          </form>
          <p class="empty-hint">
            "Removes Circus's PostgreSQL failed-path skip records and requeues cached failures \
             for those derivations. If the cache was already cleared, it requeues all remaining "
            <code>"cached_failure"</code>
            " builds. This does not delete Nix store paths, build logs, or historical build rows."
          </p>
        </div>
      </section>

      <section class="panel">
        <div class="panel-header"><h2>"Pinned Build Outputs"</h2></div>
        flash(notice: notices.pinned)
        if overview.pinned_outputs.is_empty() {
          empty(
            title: "No pinned build outputs",
            hint: "Builds marked keep=true will appear here with their recorded GC roots.",
          )
        } else {
          <div class="table-wrap compact-table-wrap">
            <table>
              <thead>
                <tr>
                  <th>"Build"</th>
                  <th>"Output"</th>
                  <th>"System"</th>
                  <th>"Status"</th>
                  <th>"Store Path"</th>
                  <th>"GC Root"</th>
                  <th>"Recorded"</th>
                  <th class="row-actions">"Actions"</th>
                </tr>
              </thead>
              <tbody>
                #[key((output.build_id.as_u128(), output.product_name.clone()))]
                for output in overview.pinned_outputs {
                  <tr>
                    <td>
                      <a href=(format!("/build/{}", output.build_id))>(output.job_name)</a>
                    </td>
                    <td>(output.product_name)</td>
                    <td>(output.system)</td>
                    <td>(output.status)</td>
                    <td><code class="path-short">(output.path)</code></td>
                    <td>
                      if output.gc_root_path.is_empty() {
                        "-"
                      } else {
                        <code class="path-short">(output.gc_root_path)</code>
                      }
                    </td>
                    <td>local_time(at: output.product_created_at)</td>
                    <td class="row-actions">
                      <form
                        method="POST"
                        action=(format!("/admin/pinned-builds/{}/unpin", output.build_id))
                        class="inline-form"
                      >
                        csrf(token: token)
                        confirm_button(
                          prompt: "Unpin this build? All of its outputs become eligible for GC once their roots age out.",
                          class: "btn btn-small btn-danger",
                          label: "Unpin Build",
                        )
                      </form>
                    </td>
                  </tr>
                }
              </tbody>
            </table>
          </div>
        }
      </section>

      <section class="panel">
        <div class="panel-header"><h2>"API Keys"</h2></div>
        <div class="panel-body">
          <form method="POST" action="/admin/api-keys" class="filter-form">
            csrf(token: token)
            <label>"Name: " <input type="text" name="name" required=(true)></label>
            <label>
              "Role: "
              <select name="role">
                for role in KEY_ROLES {
                  <option value=(role.as_str()) selected=(role == GlobalRole::ReadOnly)>
                    (role.as_str())
                  </option>
                }
              </select>
            </label>
            <button type="submit" class="btn">"Create Key"</button>
          </form>
          if let Some(key) = notices.new_key {
            <div class="flash-message flash-success">
              "Key created: " <code>(key.as_str())</code> " " copy_button(text: &key)
              <br>
              "Copy this now, it will not be shown again."
            </div>
          }
          flash(notice: notices.keys)
        </div>
        if overview.api_keys.is_empty() {
          empty(title: "No API keys", hint: "Create an API key above to enable API access.")
        } else {
          <div class="table-wrap compact-table-wrap">
            <table>
              <thead>
                <tr>
                  <th>"Name"</th>
                  <th>"Role"</th>
                  <th>"Created"</th>
                  <th>"Last Used"</th>
                  <th class="row-actions">"Actions"</th>
                </tr>
              </thead>
              <tbody>
                #[key(key.id.as_u128())]
                for key in overview.api_keys {
                  <tr>
                    <td>(key.name)</td>
                    <td><span class="badge badge-pending">(key.role)</span></td>
                    <td>local_time(at: key.created_at)</td>
                    <td>
                      match key.last_used_at {
                        Some(at) => local_time(at: at),
                        None => "Never",
                      }
                    </td>
                    <td class="row-actions">
                      <form
                        method="POST"
                        action=(format!("/admin/api-keys/{}/delete", key.id))
                        class="inline-form"
                      >
                        csrf(token: token)
                        confirm_button(
                          prompt: "Delete this API key?",
                          class: "btn btn-small btn-danger",
                          label: "Delete",
                        )
                      </form>
                    </td>
                  </tr>
                }
              </tbody>
            </table>
          </div>
        }
      </section>

      <section class="panel">
        <div class="panel-header"><h2>"Configuration"</h2></div>
        <div class="panel-body">
          if !overview.config_editable {
            <div class="flash-message flash-error">(overview.config_read_only_reason)</div>
          }
          flash(notice: notices.config)
          <form method="POST" action="/admin/config">
            csrf(token: token)
            <div class="form-group">
              <label for="config-contents">"TOML"</label>
              <textarea
                id="config-contents"
                name="contents"
                rows="18"
                spellcheck="false"
                readonly=(!overview.config_editable)
              >
                (overview.config_contents)
              </textarea>
            </div>
            if overview.config_editable {
              <button type="submit" class="btn">"Save Configuration"</button>
            }
          </form>
        </div>
      </section>

      <section class="panel">
        <div class="panel-header"><h2>"Notification Retry Tasks"</h2></div>
        flash(notice: notices.tasks)
        if overview.notification_tasks.is_empty() {
          empty(
            title: "No notification tasks",
            hint: "Queued notification deliveries and retries will appear here.",
          )
        } else {
          <div class="table-wrap compact-table-wrap">
            <table>
              <thead>
                <tr>
                  <th>"Type"</th>
                  <th>"Status"</th>
                  <th>"Attempts"</th>
                  <th>"Next Retry"</th>
                  <th>"Created"</th>
                  <th>"Error"</th>
                  <th class="row-actions">"Actions"</th>
                </tr>
              </thead>
              <tbody>
                #[key(task.id.as_u128())]
                for task in overview.notification_tasks {
                  <tr>
                    <td>(task.notification_type)</td>
                    <td>
                      <span class=(format!("badge badge-{}", task.status))>(task.status.as_str())</span>
                    </td>
                    <td>(task.attempts) "/" (task.max_attempts)</td>
                    <td>local_time(at: task.next_retry_at)</td>
                    <td>local_time(at: task.created_at)</td>
                    <td>
                      if task.last_error.is_empty() { "-" } else { (task.last_error) }
                    </td>
                    <td class="row-actions">
                      if task.status == "failed" {
                        <form
                          method="POST"
                          action=(format!("/admin/notification-tasks/{}/retry", task.id))
                          class="inline-form"
                        >
                          csrf(token: token)
                          <button type="submit" class="btn btn-small btn-secondary">"Retry"</button>
                        </form>
                      } else {
                        "-"
                      }
                    </td>
                  </tr>
                }
              </tbody>
            </table>
          </div>
        }
      </section>

      <section class="panel" id="agents">
        <div class="panel-header"><h2>"Agents"</h2></div>
        if overview.agents.is_empty() {
          empty(
            title: "No agents registered",
            hint: "Agents connect and register themselves. They will appear here once connected.",
          )
        } else {
          <div class="table-wrap compact-table-wrap">
            <table class="agents-table">
              <colgroup>
                <col class="agents-col-name">
                <col class="agents-col-host">
                <col class="agents-col-systems">
                <col class="agents-col-jobs">
                <col class="agents-col-status">
                <col class="agents-col-succeeded">
                <col class="agents-col-failed">
                <col class="agents-col-last-seen">
              </colgroup>
              <thead>
                <tr>
                  for header in overview.agent_sort_headers {
                    <th aria-sort=(header.aria_sort)>
                      <a
                        class=(if header.active { "sort-link is-active" } else { "sort-link" })
                        href=(header.href)
                      >
                        <span>(header.label)</span>
                        <span class=(format!("sort-indicator sort-{}", header.indicator)) aria-hidden="true"></span>
                      </a>
                    </th>
                  }
                </tr>
              </thead>
              <tbody>
                #[key(agent.machine_id.as_u128())]
                for agent in overview.agents {
                  <tr>
                    <td>(agent.name)</td>
                    <td>(agent.hostname)</td>
                    <td>(agent.systems)</td>
                    <td>(agent.current_jobs) "/" (agent.max_jobs)</td>
                    <td>
                      if agent.connected {
                        <span class="badge badge-completed">"Connected"</span>
                      } else {
                        <span class="badge badge-failed">"Disconnected"</span>
                      }
                    </td>
                    <td>(agent.builds_succeeded)</td>
                    <td>(agent.builds_failed)</td>
                    <td>
                      match agent.last_seen {
                        Some(at) => local_time(at: at),
                        None => "Never",
                      }
                    </td>
                  </tr>
                }
              </tbody>
            </table>
          </div>
        }
      </section>
    )
  })
}
