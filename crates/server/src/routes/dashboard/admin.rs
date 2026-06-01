//! Admin-only dashboard pages and the mutating forms that live on them:
//! the admin overview, news creation/deletion, project-notification
//! configuration, and the user-management page. The first thing each
//! mutating handler does is call `is_admin` and `check_csrf`, in that
//! order, so a non-admin attempting to forge a request never reaches the
//! database.

use askama::Template;
use axum::{
  Form,
  extract::{Path, Query, State},
  http::{Extensions, StatusCode},
  response::{Html, IntoResponse, Redirect, Response},
};
use circus_common::models::{CreateNotificationConfig, SystemStatus, UserType};
use uuid::Uuid;

use super::{
  csrf::{check_csrf, csrf_from},
  pages::PageParams,
  shared::{
    ApiKeyView,
    DashboardPage,
    UserView,
    auth_name,
    enforce_page_access,
    is_admin,
  },
  templates::{
    AdminTemplate,
    BuilderView,
    NewsTemplate,
    NotificationTaskView,
    NotificationsTemplate,
    PinnedOutputView,
    UsersTemplate,
  },
};
use crate::state::AppState;

// ---------- Admin overview ----------

pub(super) async fn admin_page(
  State(state): State<AppState>,
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  if !is_admin(&extensions) {
    let target = if auth_name(&extensions).is_empty() {
      "/login"
    } else {
      "/"
    };
    return Err(Redirect::to(target).into_response());
  }

  let pool = &state.pool;

  let projects: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects")
    .fetch_one(pool)
    .await
    .unwrap_or((0,));
  let jobsets: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM jobsets")
    .fetch_one(pool)
    .await
    .unwrap_or((0,));
  let evaluations: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM evaluations")
    .fetch_one(pool)
    .await
    .unwrap_or((0,));
  let build_stats = circus_common::repo::builds::get_stats(pool)
    .await
    .unwrap_or_default();
  let builders_count = circus_common::repo::remote_builders::count(pool)
    .await
    .unwrap_or(0);
  let channels: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM channels")
    .fetch_one(pool)
    .await
    .unwrap_or((0,));

  let status = SystemStatus {
    projects_count:    projects.0,
    jobsets_count:     jobsets.0,
    evaluations_count: evaluations.0,
    builds_pending:    build_stats.pending_builds.unwrap_or(0),
    builds_running:    build_stats.running_builds.unwrap_or(0),
    builds_completed:  build_stats.completed_builds.unwrap_or(0),
    builds_failed:     build_stats.failed_builds.unwrap_or(0),
    remote_builders:   builders_count,
    channels_count:    channels.0,
  };
  let raw_builders = circus_common::repo::remote_builders::list(pool)
    .await
    .unwrap_or_default();

  // Get running builds to calculate builder load
  let running_builds = circus_common::repo::builds::list_filtered(
    pool,
    None,
    Some("running"),
    None,
    None,
    1000,
    0,
  )
  .await
  .unwrap_or_default();

  // Count builds per builder
  let mut builds_per_builder: std::collections::HashMap<Uuid, i64> =
    std::collections::HashMap::new();
  for build in &running_builds {
    if let Some(builder_id) = build.builder_id {
      *builds_per_builder.entry(builder_id).or_insert(0) += 1;
    }
  }

  // Convert to BuilderView with load info
  let builders: Vec<BuilderView> = raw_builders
    .into_iter()
    .map(|b| {
      let current_builds = *builds_per_builder.get(&b.id).unwrap_or(&0);
      let load_percent = if b.max_jobs > 0 {
        (current_builds * 100) / i64::from(b.max_jobs)
      } else {
        0
      };
      BuilderView {
        id: b.id,
        name: b.name,
        ssh_uri: b.ssh_uri,
        systems: b.systems.join(", "),
        max_jobs: b.max_jobs,
        enabled: b.enabled,
        current_builds,
        load_percent,
        last_activity: b.created_at.format("%Y-%m-%d").to_string(),
      }
    })
    .collect();

  // Fetch API keys for admin view
  let keys = circus_common::repo::api_keys::list(pool)
    .await
    .unwrap_or_default();
  let api_keys: Vec<ApiKeyView> = keys
    .into_iter()
    .map(|k| {
      ApiKeyView {
        id:           k.id,
        name:         k.name,
        role:         k.role,
        created_at:   k.created_at.format("%Y-%m-%d %H:%M").to_string(),
        last_used_at: k.last_used_at.map_or_else(
          || "Never".to_string(),
          |t| t.format("%Y-%m-%d %H:%M").to_string(),
        ),
      }
    })
    .collect();
  let notification_tasks =
    circus_common::repo::notification_tasks::list_recent(pool, 25)
      .await
      .unwrap_or_default()
      .into_iter()
      .map(|task| {
        NotificationTaskView {
          id:                task.id,
          notification_type: task.notification_type,
          status:            format!("{:?}", task.status).to_lowercase(),
          attempts:          task.attempts,
          max_attempts:      task.max_attempts,
          next_retry_at:     task
            .next_retry_at
            .format("%Y-%m-%d %H:%M")
            .to_string(),
          last_error:        task.last_error.unwrap_or_default(),
          created_at:        task
            .created_at
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        }
      })
      .collect();
  let pinned_outputs =
    circus_common::repo::build_products::list_pinned(pool, 100, 0)
      .await
      .unwrap_or_default()
      .into_iter()
      .map(|product| {
        PinnedOutputView {
          build_id:           product.build_id,
          product_id:         product.product_id,
          job_name:           product.job_name,
          system:             product.system,
          status:             product.status.to_string(),
          product_name:       product.product_name,
          path:               product.path,
          gc_root_path:       product.gc_root_path.unwrap_or_default(),
          product_created_at: product
            .product_created_at
            .format("%Y-%m-%d %H:%M")
            .to_string(),
        }
      })
      .collect();
  let config_path = std::env::var("CIRCUS_CONFIG_FILE")
    .unwrap_or_else(|_| "circus.toml".to_string());
  let config_contents = match tokio::fs::read_to_string(&config_path).await {
    Ok(contents) => {
      circus_common::config::Config::from_toml_with_defaults(&contents)
        .ok()
        .and_then(|config| toml::to_string_pretty(&config).ok())
        .unwrap_or(contents)
    },
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
      toml::to_string_pretty(&circus_common::config::Config::default())
        .unwrap_or_default()
    },
    Err(_) => String::new(),
  };
  let config_editable = state.config.server.config_editor_enabled;
  let config_read_only_reason = if config_editable {
    String::new()
  } else {
    "Config editor is disabled by server configuration".to_string()
  };

  let tmpl = AdminTemplate {
    status,
    builders,
    api_keys,
    notification_tasks,
    pinned_outputs,
    config_path,
    config_contents,
    config_editable,
    config_read_only_reason,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
    csrf_token: csrf_from(&extensions),
  };
  Ok(Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  ))
}

// ---------- Users page ----------

pub(super) async fn users_page(
  State(state): State<AppState>,
  Query(params): Query<PageParams>,
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  // Only admins can view user list (contains PII like emails)
  if !is_admin(&extensions) {
    return Err(Redirect::to("/").into_response());
  }

  let limit = params.limit.unwrap_or(50).clamp(1, 200);
  let offset = params.offset.unwrap_or(0).max(0);

  let users_list = circus_common::repo::users::list(&state.pool, limit, offset)
    .await
    .unwrap_or_default();
  let total = circus_common::repo::users::count(&state.pool)
    .await
    .unwrap_or(0);

  let users: Vec<UserView> = users_list
    .into_iter()
    .map(|u| {
      let user_type = match u.user_type {
        UserType::Local => "Local",
        UserType::Github => "GitHub",
        UserType::Google => "Google",
        UserType::Ldap => "LDAP",
      };
      UserView {
        id:            u.id,
        username:      u.username,
        email:         u.email,
        role:          u.role,
        user_type:     user_type.to_string(),
        enabled:       u.enabled,
        last_login_at: u.last_login_at.map_or_else(
          || "Never".to_string(),
          |t| t.format("%Y-%m-%d %H:%M").to_string(),
        ),
      }
    })
    .collect();

  let total_pages = (total + limit - 1) / limit.max(1);
  let page = offset / limit.max(1) + 1;

  let tmpl = UsersTemplate {
    users,
    limit,
    has_prev: offset > 0,
    has_next: offset + limit < total,
    prev_offset: (offset - limit).max(0),
    next_offset: offset + limit,
    page,
    total_pages,
    is_admin: true, // Already checked above
    auth_name: auth_name(&extensions),
    csrf_token: csrf_from(&extensions),
  };
  Ok(Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  ))
}

// ---------- News ----------

pub(super) async fn news_page(
  State(state): State<AppState>,
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  enforce_page_access(&state.config.server, &extensions, DashboardPage::News)?;
  let items = circus_common::repo::news::list(&state.pool, 50, 0)
    .await
    .unwrap_or_default();
  let tmpl = NewsTemplate {
    items,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
    csrf_token: csrf_from(&extensions),
  };
  Ok(Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  ))
}

#[derive(serde::Deserialize)]
pub(super) struct NewsCreateForm {
  title:      String,
  content:    String,
  csrf_token: String,
}

pub(super) async fn news_create(
  State(state): State<AppState>,
  extensions: Extensions,
  Form(form): Form<NewsCreateForm>,
) -> Response {
  if !is_admin(&extensions) {
    return StatusCode::FORBIDDEN.into_response();
  }
  if let Err(e) = check_csrf(&extensions, &form.csrf_token) {
    return e;
  }
  if form.title.trim().is_empty() {
    return (StatusCode::BAD_REQUEST, "Title is required").into_response();
  }
  if let Err(e) = circus_common::repo::news::create(
    &state.pool,
    circus_common::models::CreateNewsItem {
      title:      form.title.trim().to_string(),
      content:    form.content,
      created_by: None,
    },
  )
  .await
  {
    tracing::warn!("Failed to create news item: {e}");
    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
  }
  Redirect::to("/news").into_response()
}

pub(super) async fn news_delete(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  extensions: Extensions,
  Form(form): Form<CsrfOnlyForm>,
) -> Response {
  if !is_admin(&extensions) {
    return StatusCode::FORBIDDEN.into_response();
  }
  if let Err(e) = check_csrf(&extensions, &form.csrf_token) {
    return e;
  }
  if let Err(e) = circus_common::repo::news::delete(&state.pool, id).await {
    tracing::warn!(id = %id, "Failed to delete news item: {e}");
  }
  Redirect::to("/news").into_response()
}

// ---------- Project-scoped notification configs ----------

#[derive(serde::Deserialize)]
pub struct NotificationCreateForm {
  pub notification_type: String,
  pub config:            String,
  pub csrf_token:        String,
}

#[derive(serde::Deserialize)]
pub struct CsrfOnlyForm {
  pub csrf_token: String,
}

#[derive(serde::Deserialize)]
pub struct EvaluationVisibilityForm {
  pub hidden:     bool,
  pub return_to:  Option<String>,
  pub csrf_token: String,
}

fn safe_redirect_target(target: Option<String>, fallback: String) -> String {
  target
    .filter(|t| t.starts_with('/') && !t.starts_with("//"))
    .unwrap_or(fallback)
}

pub(super) async fn jobset_delete(
  State(state): State<AppState>,
  Path(jobset_id): Path<Uuid>,
  extensions: Extensions,
  Form(form): Form<CsrfOnlyForm>,
) -> Result<Redirect, Response> {
  if !is_admin(&extensions) {
    return Err((StatusCode::FORBIDDEN, "Admin required").into_response());
  }
  check_csrf(&extensions, &form.csrf_token)?;
  let jobset = circus_common::repo::jobsets::get(&state.pool, jobset_id)
    .await
    .map_err(|e| {
      (StatusCode::NOT_FOUND, format!("Jobset not found: {e}")).into_response()
    })?;
  let project_id = jobset.project_id;
  circus_common::repo::jobsets::delete(&state.pool, jobset_id)
    .await
    .map_err(|e| {
      (StatusCode::BAD_REQUEST, format!("Delete failed: {e}")).into_response()
    })?;
  Ok(Redirect::to(&format!("/project/{project_id}")))
}

pub(super) async fn evaluation_visibility(
  State(state): State<AppState>,
  Path(evaluation_id): Path<Uuid>,
  extensions: Extensions,
  Form(form): Form<EvaluationVisibilityForm>,
) -> Result<Redirect, Response> {
  if !is_admin(&extensions) {
    return Err((StatusCode::FORBIDDEN, "Admin required").into_response());
  }
  check_csrf(&extensions, &form.csrf_token)?;
  circus_common::repo::evaluations::set_hidden(
    &state.pool,
    evaluation_id,
    form.hidden,
  )
  .await
  .map_err(|e| {
    (
      StatusCode::BAD_REQUEST,
      format!("Visibility update failed: {e}"),
    )
      .into_response()
  })?;
  let target = safe_redirect_target(
    form.return_to,
    format!("/evaluation/{evaluation_id}"),
  );
  Ok(Redirect::to(&target))
}

pub(super) async fn notifications_page(
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  extensions: Extensions,
) -> Result<Html<String>, Response> {
  if !is_admin(&extensions) {
    let target = if auth_name(&extensions).is_empty() {
      "/login"
    } else {
      "/projects"
    };
    return Err(Redirect::to(target).into_response());
  }

  let project = circus_common::repo::projects::get(&state.pool, project_id)
    .await
    .map_err(|_| Redirect::to("/projects").into_response())?;
  let configs = circus_common::repo::notification_configs::list_for_project(
    &state.pool,
    project_id,
  )
  .await
  .unwrap_or_default();
  let tmpl = NotificationsTemplate {
    project,
    configs,
    is_admin: is_admin(&extensions),
    auth_name: auth_name(&extensions),
    csrf_token: csrf_from(&extensions),
  };
  Ok(Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  ))
}

pub(super) async fn notifications_create(
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  extensions: Extensions,
  Form(form): Form<NotificationCreateForm>,
) -> Result<Redirect, Response> {
  if !is_admin(&extensions) {
    return Err((StatusCode::FORBIDDEN, "Admin required").into_response());
  }
  check_csrf(&extensions, &form.csrf_token)?;
  let parsed: serde_json::Value = serde_json::from_str(form.config.trim())
    .map_err(|e| {
      (StatusCode::BAD_REQUEST, format!("Invalid JSON: {e}")).into_response()
    })?;
  if !parsed.is_object() {
    return Err(
      (StatusCode::BAD_REQUEST, "Config must be a JSON object").into_response(),
    );
  }
  let allowed_types = [
    "webhook",
    "github_status",
    "gitea_status",
    "gitlab_status",
    "email",
    "slack",
  ];
  if !allowed_types.contains(&form.notification_type.as_str()) {
    return Err(
      (StatusCode::BAD_REQUEST, "Unknown notification type").into_response(),
    );
  }

  // SSRF guard: outbound webhooks must point at public HTTP(S) hosts.
  let url_field = match form.notification_type.as_str() {
    "webhook" => Some("url"),
    "slack" => Some("webhook_url"),
    _ => None,
  };
  if let Some(field) = url_field {
    let url_str =
      parsed.get(field).and_then(|v| v.as_str()).ok_or_else(|| {
        (
          StatusCode::BAD_REQUEST,
          format!("Missing '{field}' in config"),
        )
          .into_response()
      })?;
    circus_common::validate::validate_webhook_url(url_str).map_err(|e| {
      (StatusCode::BAD_REQUEST, format!("Invalid URL: {e}")).into_response()
    })?;
  }

  circus_common::repo::notification_configs::create(
    &state.pool,
    CreateNotificationConfig {
      project_id,
      notification_type: form.notification_type,
      config: parsed,
    },
  )
  .await
  .map_err(|e| {
    (StatusCode::BAD_REQUEST, format!("Create failed: {e}")).into_response()
  })?;

  Ok(Redirect::to(&format!(
    "/project/{project_id}/notifications"
  )))
}

pub(super) async fn notifications_delete(
  State(state): State<AppState>,
  Path((project_id, config_id)): Path<(Uuid, Uuid)>,
  extensions: Extensions,
  Form(form): Form<CsrfOnlyForm>,
) -> Result<Redirect, Response> {
  if !is_admin(&extensions) {
    return Err((StatusCode::FORBIDDEN, "Admin required").into_response());
  }
  check_csrf(&extensions, &form.csrf_token)?;
  circus_common::repo::notification_configs::delete_for_project(
    &state.pool,
    project_id,
    config_id,
  )
  .await
  .map_err(|e| {
    (StatusCode::NOT_FOUND, format!("Delete failed: {e}")).into_response()
  })?;
  Ok(Redirect::to(&format!(
    "/project/{project_id}/notifications"
  )))
}
