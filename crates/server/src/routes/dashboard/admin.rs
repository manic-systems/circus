//! Admin-only dashboard pages and the mutating forms that live on them:
//! the admin overview, news creation/deletion, project-notification
//! configuration, and the user-management page. The first thing each
//! mutating handler does is call `is_admin` and `check_csrf`, in that
//! order, so a non-admin attempting to forge a request never reaches the
//! database.

use std::{cmp::Ordering, collections::HashMap, env};

use axum::{
  Form,
  extract::{Path, Query, State},
  http::StatusCode,
  response::{Html, IntoResponse, Redirect, Response},
};
use circus_common::models::{
  CreateNotificationConfig,
  NotificationType,
  SystemStatus,
  UserType,
};
use tokio::fs;
use uuid::Uuid;

use super::{
  pages::PageParams,
  shared::{
    ApiKeyView,
    DashboardContext,
    DashboardPage,
    Pagination,
    RenderExt,
    UserView,
    enforce_page_access,
  },
  templates::{
    AdminTemplate,
    AgentView,
    BuilderView,
    NewsTemplate,
    NotificationTaskView,
    NotificationsTemplate,
    PinnedOutputView,
    SortHeaderView,
    UiTemplateConfig,
    UsersTemplate,
  },
};
use crate::{permissions::Permission, state::AppState};

fn ui_config(state: &AppState) -> UiTemplateConfig {
  UiTemplateConfig::from_config(&state.config.ui)
}

#[derive(Default, serde::Deserialize)]
pub(super) struct AdminParams {
  agent_sort: Option<String>,
  agent_dir:  Option<String>,
  gc:         Option<String>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum AgentSort {
  Name,
  Host,
  Systems,
  Jobs,
  Status,
  Succeeded,
  Failed,
  LastSeen,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum SortDirection {
  Asc,
  Desc,
}

const AGENT_SORT_COLUMNS: [(AgentSort, &str); 8] = [
  (AgentSort::Name, "Name"),
  (AgentSort::Host, "Host"),
  (AgentSort::Systems, "Systems"),
  (AgentSort::Jobs, "Jobs"),
  (AgentSort::Status, "Status"),
  (AgentSort::Succeeded, "Succeeded"),
  (AgentSort::Failed, "Failed"),
  (AgentSort::LastSeen, "Last Seen"),
];

impl AgentSort {
  fn from_param(param: Option<&str>) -> Option<Self> {
    match param {
      Some("name") => Some(Self::Name),
      Some("host") => Some(Self::Host),
      Some("systems") => Some(Self::Systems),
      Some("jobs") => Some(Self::Jobs),
      Some("status") => Some(Self::Status),
      Some("succeeded") => Some(Self::Succeeded),
      Some("failed") => Some(Self::Failed),
      Some("last_seen") => Some(Self::LastSeen),
      _ => None,
    }
  }

  const fn as_param(self) -> &'static str {
    match self {
      Self::Name => "name",
      Self::Host => "host",
      Self::Systems => "systems",
      Self::Jobs => "jobs",
      Self::Status => "status",
      Self::Succeeded => "succeeded",
      Self::Failed => "failed",
      Self::LastSeen => "last_seen",
    }
  }

  const fn default_direction(self) -> SortDirection {
    match self {
      Self::Name | Self::Host | Self::Systems => SortDirection::Asc,
      Self::Jobs
      | Self::Status
      | Self::Succeeded
      | Self::Failed
      | Self::LastSeen => SortDirection::Desc,
    }
  }
}

impl SortDirection {
  fn from_param(param: Option<&str>, sort: AgentSort) -> Self {
    match param {
      Some("asc") => Self::Asc,
      Some("desc") => Self::Desc,
      _ => sort.default_direction(),
    }
  }

  const fn as_param(self) -> &'static str {
    match self {
      Self::Asc => "asc",
      Self::Desc => "desc",
    }
  }

  const fn toggle(self) -> Self {
    match self {
      Self::Asc => Self::Desc,
      Self::Desc => Self::Asc,
    }
  }
}

fn agent_sort_headers(
  active_sort: AgentSort,
  active_dir: SortDirection,
) -> Vec<SortHeaderView> {
  AGENT_SORT_COLUMNS
    .iter()
    .map(|(sort, label)| {
      let active = active_sort == *sort;
      let next_dir = if active {
        active_dir.toggle()
      } else {
        sort.default_direction()
      };
      SortHeaderView {
        key: sort.as_param().to_string(),
        label: (*label).to_string(),
        href: format!(
          "/admin?agent_sort={}&agent_dir={}#agents",
          sort.as_param(),
          next_dir.as_param(),
        ),
        default_dir: sort.default_direction().as_param().to_string(),
        active,
        indicator: if active {
          active_dir.as_param().to_string()
        } else {
          String::new()
        },
        aria_sort: if active {
          match active_dir {
            SortDirection::Asc => "ascending",
            SortDirection::Desc => "descending",
          }
        } else {
          "none"
        }
        .to_string(),
      }
    })
    .collect()
}

fn sort_agents(agents: &mut [AgentView], sort: AgentSort, dir: SortDirection) {
  agents.sort_by(|a, b| compare_agents_by_sort(a, b, sort, dir));
}

fn compare_agents_by_sort(
  a: &AgentView,
  b: &AgentView,
  sort: AgentSort,
  dir: SortDirection,
) -> Ordering {
  let primary = match sort {
    AgentSort::Name => compare_text(&a.name, &b.name),
    AgentSort::Host => compare_text(&a.hostname, &b.hostname),
    AgentSort::Systems => compare_text(&a.systems, &b.systems),
    AgentSort::Jobs => {
      a.current_jobs
        .cmp(&b.current_jobs)
        .then_with(|| a.max_jobs.cmp(&b.max_jobs))
    },
    AgentSort::Status => a.connected.cmp(&b.connected),
    AgentSort::Succeeded => a.builds_succeeded.cmp(&b.builds_succeeded),
    AgentSort::Failed => a.builds_failed.cmp(&b.builds_failed),
    AgentSort::LastSeen => a.last_seen_sort.cmp(&b.last_seen_sort),
  };

  apply_direction(primary, dir)
    .then_with(|| {
      match sort {
        AgentSort::Status => {
          apply_direction(
            a.last_seen_sort.cmp(&b.last_seen_sort),
            SortDirection::Desc,
          )
        },
        _ => Ordering::Equal,
      }
    })
    .then_with(|| compare_agent_identity(a, b))
}

fn compare_agent_identity(a: &AgentView, b: &AgentView) -> Ordering {
  compare_text(&a.name, &b.name)
    .then_with(|| compare_text(&a.hostname, &b.hostname))
    .then_with(|| a.machine_id.as_bytes().cmp(b.machine_id.as_bytes()))
}

fn compare_text(a: &str, b: &str) -> Ordering {
  a.to_lowercase()
    .cmp(&b.to_lowercase())
    .then_with(|| a.cmp(b))
}

const fn apply_direction(ordering: Ordering, dir: SortDirection) -> Ordering {
  match dir {
    SortDirection::Asc => ordering,
    SortDirection::Desc => ordering.reverse(),
  }
}

/// Render the admin overview at `/admin`: system status counters, builder
/// load and last-activity, API keys, queued notification tasks, pinned
/// build outputs, and the on-disk config editor when writes are enabled.
pub(super) async fn admin_page(
  State(state): State<AppState>,
  Query(params): Query<AdminParams>,
  ctx: DashboardContext,
) -> Result<Html<String>, Response> {
  if !ctx.is_admin {
    let target = if ctx.auth_name.is_empty() {
      "/login"
    } else {
      "/"
    };
    return Err(Redirect::to(target).into_response());
  }

  let pool = &state.pool;

  let projects = circus_common::repo::projects::count(pool)
    .await
    .unwrap_or(0);
  let jobsets = circus_common::repo::jobsets::count(pool).await.unwrap_or(0);
  let evaluations = circus_common::repo::evaluations::count(pool)
    .await
    .unwrap_or(0);
  let build_stats = circus_common::repo::builds::get_stats(pool)
    .await
    .unwrap_or_default();
  let builders_count = circus_common::repo::remote_builders::count(pool)
    .await
    .unwrap_or(0);
  let channels = circus_common::repo::channels::count(pool)
    .await
    .unwrap_or(0);

  let status = SystemStatus {
    projects_count:    projects,
    jobsets_count:     jobsets,
    evaluations_count: evaluations,
    builds_pending:    build_stats.pending_builds.unwrap_or(0),
    builds_running:    build_stats.running_builds.unwrap_or(0),
    builds_completed:  build_stats.completed_builds.unwrap_or(0),
    builds_failed:     build_stats.failed_builds.unwrap_or(0),
    remote_builders:   builders_count,
    channels_count:    channels,
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
  let mut builds_per_builder: HashMap<Uuid, i64> = HashMap::new();
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

  // Fetch connected agents
  let raw_sessions = circus_common::repo::builder_sessions::list(pool)
    .await
    .unwrap_or_default();
  let agent_sort = AgentSort::from_param(params.agent_sort.as_deref())
    .unwrap_or(AgentSort::Name);
  let agent_dir =
    SortDirection::from_param(params.agent_dir.as_deref(), agent_sort);
  let mut agents = raw_sessions
    .into_iter()
    .map(|s| {
      let last_seen = s.last_seen;
      let last_seen_display = last_seen.as_ref().map_or_else(
        || "Never".to_string(),
        |t| t.format("%Y-%m-%d %H:%M").to_string(),
      );
      let last_seen_sort = last_seen.map_or(0, |t| t.timestamp());
      AgentView {
        machine_id: s.machine_id,
        name: s.name,
        hostname: s.hostname,
        systems: s.systems.join(", "),
        max_jobs: s.max_jobs,
        current_jobs: s.current_jobs,
        connected: s.connected,
        builds_succeeded: s.builds_succeeded,
        builds_failed: s.builds_failed,
        last_seen: last_seen_display,
        last_seen_sort,
      }
    })
    .collect::<Vec<AgentView>>();
  sort_agents(&mut agents, agent_sort, agent_dir);
  let agent_sort_headers = agent_sort_headers(agent_sort, agent_dir);

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
        role:         k.role.to_string(),
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
          notification_type: task.notification_type.to_string(),
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
  let config_path = env::var("CIRCUS_CONFIG_FILE").unwrap_or_default();
  let config_contents = if config_path.is_empty() {
    String::new()
  } else {
    fs::read_to_string(&config_path).await.map_or_else(
      |_| String::new(),
      |contents| {
        circus_config::Config::from_toml_with_defaults(&contents)
          .ok()
          .and_then(|config| {
            let mut value = toml::Value::try_from(&config).ok()?;
            circus_config::redact_secrets(&mut value);
            toml::to_string_pretty(&value).ok()
          })
          .unwrap_or(contents)
      },
    )
  };
  let config_editable =
    state.config.server.config_editor_enabled && !config_path.is_empty();
  let config_read_only_reason = if config_editable {
    String::new()
  } else if config_path.is_empty() {
    "CIRCUS_CONFIG_FILE is not set; no config file is available".to_string()
  } else {
    "Config editor is disabled by server configuration".to_string()
  };

  let tmpl = AdminTemplate {
    ui: ui_config(&state),
    status,
    builders,
    agents,
    agent_sort_headers,
    agent_sort_key: agent_sort.as_param().to_string(),
    agent_sort_dir: agent_dir.as_param().to_string(),
    api_keys,
    notification_tasks,
    pinned_outputs,
    config_path,
    config_contents,
    config_editable,
    config_read_only_reason,
    gc_enabled: state.config.gc.enabled,
    gc_requested: params.gc.as_deref() == Some("requested"),
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name.clone(),
    csrf_token: ctx.csrf_token.clone(),
  };
  tmpl.render_html_or_500()
}

/// Ask the queue runner to run a GC cycle now. The runner's GC loop listens
/// on [`circus_common::pg_notify::CHANNEL_GC_REQUESTED`] and runs root
/// cleanup plus `nix-collect-garbage`; results land in the runner's logs.
pub(super) async fn store_gc(
  State(state): State<AppState>,
  ctx: DashboardContext,
  Form(form): Form<CsrfOnlyForm>,
) -> Response {
  if !ctx.is_admin {
    return StatusCode::FORBIDDEN.into_response();
  }
  if let Err(e) = ctx.check_csrf(&form.csrf_token) {
    return e;
  }
  if !state.config.gc.enabled {
    return (StatusCode::CONFLICT, "Garbage collection is disabled")
      .into_response();
  }
  if let Err(e) = circus_common::pg_notify::notify(
    &state.pool,
    circus_common::pg_notify::CHANNEL_GC_REQUESTED,
  )
  .await
  {
    tracing::error!("Failed to request GC cycle: {e}");
    return Redirect::to("/admin?gc=error").into_response();
  }
  tracing::info!("GC cycle requested from the dashboard");
  Redirect::to("/admin?gc=requested").into_response()
}

/// Form for `POST /caches/{name}/gc`. `mode` is `all` or `stale`; `days`
/// bounds staleness for `stale` (defaults to 30).
#[derive(serde::Deserialize)]
pub struct CacheGcForm {
  pub mode:       String,
  pub days:       Option<i64>,
  pub csrf_token: String,
}

/// Delete cache entries (and their uploaded objects) for one cache scope.
/// Store paths served from the local Nix store keep their bits until the
/// runner's GC frees them; this removes them from the cache index.
pub(super) async fn cache_gc(
  State(state): State<AppState>,
  Path(name): Path<String>,
  ctx: DashboardContext,
  Form(form): Form<CacheGcForm>,
) -> Response {
  if !ctx.is_admin {
    return StatusCode::FORBIDDEN.into_response();
  }
  if let Err(e) = ctx.check_csrf(&form.csrf_token) {
    return e;
  }
  let cache =
    match crate::cache_overview::resolve_cache_ref(&state, &name).await {
      Ok(Some(cache)) => cache,
      Ok(None) => return StatusCode::NOT_FOUND.into_response(),
      Err(e) => {
        tracing::error!(cache = %name, error = %e.0, "Failed to resolve cache");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
      },
    };

  let cutoff = if form.mode == "all" {
    None
  } else {
    let days = form.days.unwrap_or(30).clamp(1, 3650);
    Some(chrono::Utc::now() - chrono::Duration::days(days))
  };

  let deleted = match circus_common::repo::narinfo_cache::delete_stale(
    &state.pool,
    cache.scope,
    cutoff,
  )
  .await
  {
    Ok(deleted) => deleted,
    Err(e) => {
      tracing::error!(cache = %name, "Cache cleanup failed: {e}");
      return Redirect::to(&format!("/caches/{name}?gc=error")).into_response();
    },
  };
  let freed: i64 = deleted.iter().map(|nar| nar.bytes.max(0)).sum();

  // Delete the backing uploaded objects. Local-store NARs have no object;
  // an S3 DELETE for a key that never existed is a successful no-op.
  let mut object_failures = 0usize;
  if let Some(presigner) =
    crate::routes::cache::uploaded_nar_presigner(&state.config)
  {
    use futures::StreamExt as _;
    let client = reqwest::Client::new();
    // Collected first: a stream borrowing `deleted` makes this handler's
    // future fail axum's higher-ranked `Handler` bound.
    #[expect(clippy::needless_collect, reason = "see comment above")]
    let requests: Vec<(String, String)> = deleted
      .iter()
      .map(|nar| {
        (
          nar.store_path.clone(),
          presigner.presign_at(
            "DELETE",
            &nar.url,
            std::time::Duration::from_mins(5),
            std::time::SystemTime::now(),
          ),
        )
      })
      .collect();
    let mut results =
      futures::stream::iter(requests.into_iter().map(|(store_path, url)| {
        let client = client.clone();
        async move { (store_path, client.delete(&url).send().await) }
      }))
      .buffer_unordered(8);
    while let Some((store_path, result)) = results.next().await {
      match result {
        Ok(resp)
          if resp.status().is_success()
            || resp.status() == reqwest::StatusCode::NOT_FOUND => {},
        Ok(resp) => {
          object_failures += 1;
          tracing::warn!(
            store_path = %store_path,
            status = %resp.status(),
            "Failed to delete cache object"
          );
        },
        Err(e) => {
          object_failures += 1;
          tracing::warn!(
            store_path = %store_path,
            "Failed to delete cache object: {e}"
          );
        },
      }
    }
  }

  tracing::info!(
    cache = %name,
    deleted = deleted.len(),
    freed,
    object_failures,
    "Cache cleanup completed"
  );
  Redirect::to(&format!(
    "/caches/{name}?gc=done&gc_deleted={}&gc_freed={freed}&\
     gc_failed={object_failures}",
    deleted.len(),
  ))
  .into_response()
}

/// Render the user-management page at `/users`. Admin-only because the
/// listing exposes emails and other account metadata.
pub(super) async fn users_page(
  State(state): State<AppState>,
  Query(params): Query<PageParams>,
  ctx: DashboardContext,
) -> Result<Html<String>, Response> {
  // Only admins can view user list (contains PII like emails)
  if !ctx.is_admin {
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
        role:          u.role.to_string(),
        user_type:     user_type.to_string(),
        enabled:       u.enabled,
        last_login_at: u.last_login_at.map_or_else(
          || "Never".to_string(),
          |t| t.format("%Y-%m-%d %H:%M").to_string(),
        ),
      }
    })
    .collect();

  let pagination = Pagination::new(total, offset, limit);

  let tmpl = UsersTemplate {
    ui: ui_config(&state),
    users,
    limit,
    has_prev: pagination.has_prev,
    has_next: pagination.has_next,
    prev_offset: pagination.prev_offset,
    next_offset: pagination.next_offset,
    page: pagination.page,
    total_pages: pagination.total_pages,
    is_admin: true, // Already checked above
    auth_name: ctx.auth_name.clone(),
    csrf_token: ctx.csrf_token.clone(),
  };
  tmpl.render_html_or_500()
}

/// Render the news page at `/news`: list of recent announcements plus,
/// for admins, the form to publish a new one.
pub(super) async fn news_page(
  State(state): State<AppState>,
  ctx: DashboardContext,
) -> Result<Html<String>, Response> {
  enforce_page_access(&state.config, &ctx, DashboardPage::News)?;
  let items = circus_common::repo::news::list(&state.pool, 50, 0)
    .await
    .unwrap_or_default();
  let tmpl = NewsTemplate {
    ui: ui_config(&state),
    items,
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name.clone(),
    csrf_token: ctx.csrf_token.clone(),
  };
  tmpl.render_html_or_500()
}

#[derive(serde::Deserialize)]
pub(super) struct NewsCreateForm {
  title:      String,
  content:    String,
  csrf_token: String,
}

pub(super) async fn news_create(
  State(state): State<AppState>,
  ctx: DashboardContext,
  Form(form): Form<NewsCreateForm>,
) -> Response {
  if !ctx.is_admin {
    return StatusCode::FORBIDDEN.into_response();
  }
  if let Err(e) = ctx.check_csrf(&form.csrf_token) {
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
  ctx: DashboardContext,
  Form(form): Form<CsrfOnlyForm>,
) -> Response {
  if !ctx.is_admin {
    return StatusCode::FORBIDDEN.into_response();
  }
  if let Err(e) = ctx.check_csrf(&form.csrf_token) {
    return e;
  }
  if let Err(e) = circus_common::repo::news::delete(&state.pool, id).await {
    tracing::warn!(id = %id, "Failed to delete news item: {e}");
  }
  Redirect::to("/news").into_response()
}

/// Form payload for `POST /project/{id}/notifications`: the kind of
/// notification (webhook, email, ...) and a JSON blob holding the
/// kind-specific configuration.
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
  ctx: DashboardContext,
  Form(form): Form<CsrfOnlyForm>,
) -> Result<Redirect, Response> {
  if !ctx.is_admin {
    return Err((StatusCode::FORBIDDEN, "Admin required").into_response());
  }
  ctx.check_csrf(&form.csrf_token)?;
  let jobset = circus_common::repo::jobsets::get(&state.pool, jobset_id)
    .await
    .map_err(|e| {
      (StatusCode::NOT_FOUND, format!("Jobset not found: {e}")).into_response()
    })?;
  crate::routes::declarative::require_project_mutable(
    &state,
    jobset.project_id,
  )
  .await
  .map_err(IntoResponse::into_response)?;
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
  ctx: DashboardContext,
  Form(form): Form<EvaluationVisibilityForm>,
) -> Result<Redirect, Response> {
  if !ctx.is_admin {
    return Err((StatusCode::FORBIDDEN, "Admin required").into_response());
  }
  ctx.check_csrf(&form.csrf_token)?;
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

pub(super) async fn evaluation_cancel(
  State(state): State<AppState>,
  Path(evaluation_id): Path<Uuid>,
  ctx: DashboardContext,
  Form(form): Form<CsrfOnlyForm>,
) -> Result<Redirect, Response> {
  ctx
    .require_permission(Permission::CancelBuild)
    .map_err(|status| {
      (status, "Cancel evaluation permission required").into_response()
    })?;
  ctx.check_csrf(&form.csrf_token)?;
  circus_common::repo::evaluations::cancel(&state.pool, evaluation_id)
    .await
    .map_err(|e| {
      (StatusCode::BAD_REQUEST, format!("Cancel failed: {e}")).into_response()
    })?
    .ok_or_else(|| {
      (StatusCode::CONFLICT, "Evaluation is not running or pending")
        .into_response()
    })?;
  Ok(Redirect::to(&format!("/evaluation/{evaluation_id}")))
}

pub(super) async fn evaluation_restart(
  State(state): State<AppState>,
  Path(evaluation_id): Path<Uuid>,
  ctx: DashboardContext,
  Form(form): Form<CsrfOnlyForm>,
) -> Result<Redirect, Response> {
  ctx
    .require_permission(Permission::RestartJobs)
    .map_err(|status| {
      (status, "Restart evaluation permission required").into_response()
    })?;
  ctx.check_csrf(&form.csrf_token)?;
  circus_common::repo::evaluations::restart(&state.pool, evaluation_id)
    .await
    .map_err(|e| {
      (StatusCode::BAD_REQUEST, format!("Restart failed: {e}")).into_response()
    })?
    .ok_or_else(|| {
      (
        StatusCode::CONFLICT,
        "Only failed, cancelled, or timed-out evaluations with active jobsets \
         can be restarted",
      )
        .into_response()
    })?;
  Ok(Redirect::to(&format!("/evaluation/{evaluation_id}")))
}

pub(super) async fn notifications_page(
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  ctx: DashboardContext,
) -> Result<Html<String>, Response> {
  if !ctx.is_admin {
    let target = if ctx.auth_name.is_empty() {
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
    ui: ui_config(&state),
    project_mutable: crate::routes::declarative::project_is_mutable(
      &state, &project,
    ),
    project,
    configs,
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name.clone(),
    csrf_token: ctx.csrf_token.clone(),
  };
  tmpl.render_html_or_500()
}

pub(super) async fn notifications_create(
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  ctx: DashboardContext,
  Form(form): Form<NotificationCreateForm>,
) -> Result<Redirect, Response> {
  if !ctx.is_admin {
    return Err((StatusCode::FORBIDDEN, "Admin required").into_response());
  }
  ctx.check_csrf(&form.csrf_token)?;
  crate::routes::declarative::require_project_mutable(&state, project_id)
    .await
    .map_err(IntoResponse::into_response)?;
  let parsed: serde_json::Value = serde_json::from_str(form.config.trim())
    .map_err(|e| {
      (StatusCode::BAD_REQUEST, format!("Invalid JSON: {e}")).into_response()
    })?;
  if !parsed.is_object() {
    return Err(
      (StatusCode::BAD_REQUEST, "Config must be a JSON object").into_response(),
    );
  }
  let notification_type = form
    .notification_type
    .parse::<NotificationType>()
    .map_err(|_| {
      (StatusCode::BAD_REQUEST, "Unknown notification type").into_response()
    })?;
  if !NotificationType::all().contains(&notification_type) {
    return Err(
      (StatusCode::BAD_REQUEST, "Unknown notification type").into_response(),
    );
  }

  // Validate (SSRF/HTTPS guard for webhook/slack URLs and type-specific shape)
  // and encrypt secret fields before storage. The repo stores the blob
  // verbatim.
  let config = circus_notification::NotificationChannel::encrypt_into_stored(
    notification_type,
    &parsed,
    state.config.server.webhook_secret_encryption_key.as_deref(),
  )
  .map_err(|e| {
    (StatusCode::BAD_REQUEST, format!("Invalid config: {e}")).into_response()
  })?;

  circus_common::repo::notification_configs::create(
    &state.pool,
    CreateNotificationConfig {
      project_id,
      notification_type,
      config,
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
  ctx: DashboardContext,
  Form(form): Form<CsrfOnlyForm>,
) -> Result<Redirect, Response> {
  if !ctx.is_admin {
    return Err((StatusCode::FORBIDDEN, "Admin required").into_response());
  }
  ctx.check_csrf(&form.csrf_token)?;
  crate::routes::declarative::require_project_mutable(&state, project_id)
    .await
    .map_err(IntoResponse::into_response)?;
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

/// Push a pending build forward in the queue. Mirrors the JSON
/// `/builds/{id}/bump` API but accepts a session-authenticated form post
/// and redirects back to the queue page so the new ordering is visible.
pub(super) async fn queue_bump(
  State(state): State<AppState>,
  Path(build_id): Path<Uuid>,
  ctx: DashboardContext,
  Form(form): Form<CsrfOnlyForm>,
) -> Result<Redirect, Response> {
  ctx
    .require_permission(Permission::BumpToFront)
    .map_err(|s| (s, "Insufficient permissions").into_response())?;
  ctx.check_csrf(&form.csrf_token)?;
  let updated =
    circus_common::repo::builds::bump_priority(&state.pool, build_id, 10)
      .await
      .map_err(|e| {
        tracing::error!(build_id = %build_id, error = %e, "Bump failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "Bump failed").into_response()
      })?;
  if updated.is_none() {
    return Err(
      (
        StatusCode::NOT_FOUND,
        "Build not found or no longer pending",
      )
        .into_response(),
    );
  }
  Ok(Redirect::to("/queue"))
}
