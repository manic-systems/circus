//! User management and per-project notification pages.

use std::collections::HashMap;

use circus_common::{
  models::{NotificationConfig, UserType},
  roles::GlobalRole,
};
use jiff::Timestamp;
use serde::Deserialize;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{error::see_other, page, path_param, query_params},
  view::{View, component, view},
};
use uuid::Uuid;

use super::super::{
  components::{confirm_button, local_time, result_code},
  layout::{admin_viewer, document, signed_in},
  shared::Pagination,
};
use crate::state::AppState;

const ROLES: &[GlobalRole] = &[
  GlobalRole::ReadOnly,
  GlobalRole::Admin,
  GlobalRole::CreateProjects,
  GlobalRole::EvalJobset,
  GlobalRole::CancelBuild,
  GlobalRole::RestartJobs,
  GlobalRole::BumpToFront,
];

/// What a user form post did, carried back to `/users` as `?done=`.
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum UserDone {
  Created,
  Enabled,
  Disabled,
  Deleted,
}

impl UserDone {
  pub const fn code(self) -> &'static str {
    match self {
      Self::Created => "created",
      Self::Enabled => "enabled",
      Self::Disabled => "disabled",
      Self::Deleted => "deleted",
    }
  }

  const fn message(self) -> &'static str {
    match self {
      Self::Created => "User created.",
      Self::Enabled => "User enabled.",
      Self::Disabled => "User disabled.",
      Self::Deleted => "User deleted.",
    }
  }
}

/// Why a user form post failed, carried back to `/users` as `?error=`.
#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum UserError {
  Username,
  Email,
  Password,
  FullName,
  Role,
  Exists,
  NotFound,
  Failed,
}

impl UserError {
  pub const fn code(self) -> &'static str {
    match self {
      Self::Username => "username",
      Self::Email => "email",
      Self::Password => "password",
      Self::FullName => "full-name",
      Self::Role => "role",
      Self::Exists => "exists",
      Self::NotFound => "not-found",
      Self::Failed => "failed",
    }
  }

  const fn message(self) -> &'static str {
    match self {
      Self::Username => {
        "Usernames are 3-32 characters of letters, numbers, dashes and \
         underscores."
      },
      Self::Email => "Enter a valid email address.",
      Self::Password => {
        "Passwords need at least 12 characters with upper and lower case \
         letters, a number and a symbol."
      },
      Self::FullName => "That full name is not allowed.",
      Self::Role => "Unknown role.",
      Self::Exists => "A user with that username or email already exists.",
      Self::NotFound => "That user no longer exists.",
      Self::Failed => "The change could not be saved.",
    }
  }

  /// Maps a validation failure to the form field it came from.
  #[must_use]
  pub fn for_field(field: &str) -> Self {
    match field {
      "username" => Self::Username,
      "email" => Self::Email,
      "password" => Self::Password,
      "full_name" => Self::FullName,
      _ => Self::Failed,
    }
  }
}

struct LinkedIdentity {
  provider: String,
  label:    String,
}

struct UserRow {
  id:            Uuid,
  username:      String,
  email:         String,
  role:          String,
  user_type:     &'static str,
  enabled:       bool,
  last_login_at: Option<Timestamp>,
  linked:        Vec<LinkedIdentity>,
}

#[query_params(error = bad_request)]
struct UsersQuery {
  limit:  Option<i64>,
  offset: Option<i64>,
  done:   Option<String>,
  error:  Option<String>,
}

#[page("/users")]
async fn users_page(cx: &Cx) -> Result<impl View> {
  let viewer = admin_viewer(cx).await?;
  let state = app_context::<AppState>(cx);
  let query = query_params::<UsersQuery>(cx)?;
  let limit = query.limit.unwrap_or(50).clamp(1, 200);
  let offset = query.offset.unwrap_or(0).max(0);
  let users = circus_common::repo::users::list(&state.pool, limit, offset)
    .await
    .unwrap_or_default();
  let total = circus_common::repo::users::count(&state.pool)
    .await
    .unwrap_or(0);
  let ids: Vec<Uuid> = users.iter().map(|user| user.id).collect();
  let mut linked =
    circus_common::repo::users::identities_for_users(&state.pool, &ids)
      .await
      .unwrap_or_else(|error| {
        tracing::warn!("failed to list linked identities: {error}");
        HashMap::new()
      });
  let rows: Vec<UserRow> = users
    .into_iter()
    .map(|user| {
      let linked = linked
        .remove(&user.id)
        .unwrap_or_default()
        .into_iter()
        .map(|provider| {
          let label = state
            .config
            .oauth
            .oidc
            .get(&provider)
            .map_or_else(|| provider.clone(), |p| p.display_name.clone());
          LinkedIdentity { provider, label }
        })
        .collect();

      UserRow {
        id: user.id,
        username: user.username,
        email: user.email,
        role: user.role.to_string(),
        user_type: match user.user_type {
          UserType::Local => "Local",
          UserType::Github => "GitHub",
          UserType::Google => "Google",
          UserType::Ldap => "LDAP",
          UserType::Oidc => "OIDC",
        },
        enabled: user.enabled,
        last_login_at: user.last_login_at,
        linked,
      }
    })
    .collect();
  let pagination = Pagination::new(total, offset, limit);
  let page_href = |offset: i64| format!("/users?limit={limit}&offset={offset}");
  let prev_href = page_href(pagination.prev_offset);
  let next_href = page_href(pagination.next_offset);
  let csrf = viewer.csrf_token.clone();

  Ok(view! {
    document(title: "Users", viewer: &viewer,
      <h1>"User Management"</h1>
      if let Some(done) = result_code::<UserDone>(query.done.as_deref()) {
        <div class="flash-message flash-success">(done.message())</div>
      }
      if let Some(error) = result_code::<UserError>(query.error.as_deref()) {
        <div class="flash-message flash-error">(error.message())</div>
      }
      create_user_form(csrf_token: &csrf)
      if rows.is_empty() {
        <div class="empty">
          <div class="empty-title">"No users"</div>
          <div class="empty-hint">"Create a user above to enable user authentication."</div>
        </div>
      } else {
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>"Username"</th>
                <th>"Email"</th>
                <th>"Role"</th>
                <th>"Type"</th>
                <th>"Enabled"</th>
                <th>"Last Login"</th>
                <th class="row-actions">"Actions"</th>
              </tr>
            </thead>
            <tbody>
              #[key(user.id.as_u128())]
              for user in rows {
                user_row(user: user, csrf_token: &csrf)
              }
            </tbody>
          </table>
        </div>
        <nav class="pagination">
          if pagination.has_prev {
            <a href=(prev_href) class="btn btn-small btn-secondary">"Previous"</a>
          }
          <span class="text-muted">"Page " (pagination.page) " of " (pagination.total_pages)</span>
          if pagination.has_next {
            <a href=(next_href) class="btn btn-small btn-secondary">"Next"</a>
          }
        </nav>
      }
    )
  })
}

#[component]
async fn create_user_form(csrf_token: &str) -> Result<impl View> {
  Ok(view! {
    <section class="panel">
      <div class="panel-header">
        <h2>"Create user"</h2>
      </div>
      <div class="panel-body">
        <form method="POST" action="/users" class="filter-form">
          <input type="hidden" name="csrf_token" value=(csrf_token)>
          <label>
            "Username: "
            <input
              type="text"
              name="username"
              required=(true)
              pattern="[a-zA-Z0-9_-]+"
              minlength="3"
              maxlength="32"
            >
          </label>
          <label>"Email: " <input type="email" name="email" required=(true)></label>
          <label>"Full Name: " <input type="text" name="full_name"></label>
          <label>
            "Password: "
            <input type="password" name="password" required=(true) minlength="12">
          </label>
          <label>
            "Role:"
            <select name="role">
              for role in ROLES {
                <option value=(role.as_str()) selected=(*role == GlobalRole::default())>
                  (role.as_str())
                </option>
              }
            </select>
          </label>
          <button type="submit" class="btn">"Create User"</button>
        </form>
      </div>
    </section>
  })
}

#[component]
async fn user_row(user: UserRow, csrf_token: &str) -> Result<impl View> {
  let toggle_label = if user.enabled { "Disable" } else { "Enable" };

  Ok(view! {
    <tr>
      <td>(user.username)</td>
      <td>(user.email)</td>
      <td><span class="badge badge-pending">(user.role)</span></td>
      <td>
        <div>(user.user_type)</div>
        for identity in user.linked {
          <div class="linked-identity">
            <span>(identity.label.clone())</span>
            <form
              class="inline-form"
              method="POST"
              action=(format!("/users/{}/unlink/{}", user.id, identity.provider))
            >
              <input type="hidden" name="csrf_token" value=(csrf_token)>
              <button
                class="btn-ghost"
                type="submit"
                title=(format!("Unlink {}", identity.label))
              >
                "Unlink"
              </button>
            </form>
          </div>
        }
      </td>
      <td>(if user.enabled { "Yes" } else { "No" })</td>
      <td>
        match user.last_login_at {
          Some(at) => local_time(at: at),
          None => "Never",
        }
      </td>
      <td class="row-actions">
        <form method="POST" action=(format!("/users/{}/enabled", user.id)) class="inline-form">
          <input type="hidden" name="csrf_token" value=(csrf_token)>
          <input type="hidden" name="enabled" value=(if user.enabled { "false" } else { "true" })>
          <button type="submit" class="btn btn-small btn-secondary">(toggle_label)</button>
        </form>
        <form method="POST" action=(format!("/users/{}/delete", user.id)) class="inline-form">
          <input type="hidden" name="csrf_token" value=(csrf_token)>
          confirm_button(
            prompt: "Delete this user? This action cannot be undone.",
            class: "btn btn-small btn-danger",
            label: "Delete",
          )
        </form>
      </td>
    </tr>
  })
}

path_param!(id: Uuid, error = not_found);

#[page("/project/{id}/notifications")]
async fn notifications_page(cx: &Cx) -> Result<impl View> {
  let (viewer, _) = signed_in(cx).await;

  if !viewer.is_admin {
    let target = if viewer.is_authenticated {
      "/projects"
    } else {
      "/login"
    };
    return Err(see_other(target).into());
  }

  let state = app_context::<AppState>(cx);
  let project_id = *path_param::<Id>(cx)?;
  let project = circus_common::repo::projects::get(&state.pool, project_id)
    .await
    .map_err(|_| see_other("/projects"))?;
  let configs = circus_common::repo::notification_configs::list_for_project(
    &state.pool,
    project_id,
  )
  .await
  .unwrap_or_default();
  let mutable = crate::routes::declarative::project_is_mutable(state, &project);
  let base = format!("/project/{project_id}/notifications");
  let title = format!("Notifications - {}", project.name);

  Ok(view! {
    document(title: &title, viewer: &viewer,
      <nav class="breadcrumbs">
        <a href="/projects">"Projects"</a>
        <span class="sep">"/"</span>
        <a href=(format!("/project/{project_id}"))>(project.name.clone())</a>
        <span class="sep">"/"</span>
        <span class="current">"Notifications"</span>
      </nav>
      <h1>"Notifications"</h1>
      <p>"Configure delivery channels for build events on this project."</p>
      if !mutable {
        <p class="flash-message">"Managed by declarative configuration. Runtime changes are disabled."</p>
      }
      <h2>"Existing"</h2>
      if configs.is_empty() {
        <div class="empty">"No notification configs yet."</div>
      } else {
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th>"Type"</th>
                <th>"Enabled"</th>
                <th>"Created"</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              #[key(config.id.as_u128())]
              for config in configs {
                notification_row(
                  config: config,
                  base: &base,
                  mutable: mutable,
                  csrf_token: &viewer.csrf_token,
                )
              }
            </tbody>
          </table>
        </div>
      }
      if mutable {
        <h2>"Add new"</h2>
        notification_form(action: &base, csrf_token: &viewer.csrf_token)
      }
    )
  })
}

#[component]
async fn notification_row(
  config: NotificationConfig,
  base: &str,
  mutable: bool,
  csrf_token: &str,
) -> Result<impl View> {
  Ok(view! {
    <tr>
      <td><code>(config.notification_type.to_string())</code></td>
      <td>(if config.enabled { "Yes" } else { "No" })</td>
      <td>local_time(at: config.created_at)</td>
      <td>
        if mutable {
          <form method="post" action=(format!("{base}/{}/delete", config.id)) class="inline">
            <input type="hidden" name="csrf_token" value=(csrf_token)>
            <button type="submit" class="btn btn-danger btn-small">"Delete"</button>
          </form>
        }
      </td>
    </tr>
  })
}

#[component]
async fn notification_form(
  action: &str,
  csrf_token: &str,
) -> Result<impl View> {
  Ok(view! {
    <div class="form-card">
      <form method="post" action=(action)>
        <input type="hidden" name="csrf_token" value=(csrf_token)>
        <div class="form-group">
          <label for="notification_type">"Type"</label>
          <select id="notification_type" name="notification_type" required=(true)>
            for kind in ["webhook", "github_status", "gitea_status", "gitlab_status", "email", "slack"] {
              <option value=(kind)>(kind)</option>
            }
          </select>
        </div>
        <div class="form-group">
          <label for="config">"Config (JSON)"</label>
          <textarea
            id="config"
            name="config"
            rows="8"
            placeholder=r#"{"webhook_url": "https://hooks.slack.com/services/..."}"#
            required=(true)
          ></textarea>
        </div>
        <button type="submit" class="btn">"Create"</button>
      </form>
    </div>
  })
}
