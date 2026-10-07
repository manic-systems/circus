//! Admin-only dashboard pages and the mutating forms that live on them:
//! the admin overview, news creation/deletion, project-notification
//! configuration, and the user-management page. The first thing each
//! mutating handler does is call `is_admin` and `check_csrf`, in that
//! order, so a non-admin attempting to forge a request never reaches the
//! database.

use std::{cmp::Ordering, env};

use axum::{
  Form,
  extract::{Path, State},
  http::StatusCode,
  response::{IntoResponse, Redirect, Response},
};
use circus_common::{
  models::{
    CreateNotificationConfig,
    CreateUser,
    NotificationType,
    SortDirection,
    SystemStatus,
    UpdateUser,
  },
  roles::GlobalRole,
  validation::{
    validate_email,
    validate_full_name,
    validate_password,
    validate_username,
  },
};
use jiff::Timestamp;
use tokio::fs;
use uuid::Uuid;

use super::{
  shared::{ApiKeyView, DashboardContext, PageError},
  templates::SortHeaderView,
  views::users::{UserDone, UserError},
};
use crate::{permissions::Permission, state::AppState};

/// Query of `/admin`: the agent table's sort and the result of the last
/// form post, which every admin action reports through a redirect.
#[derive(Default, serde::Deserialize)]
pub(super) struct AdminParams {
  pub(super) agent_sort:   Option<String>,
  pub(super) agent_dir:    Option<SortDirection>,
  pub(super) gc:           Option<String>,
  pub(super) key:          Option<String>,
  pub(super) task:         Option<String>,
  pub(super) unpinned:     Option<String>,
  pub(super) failed_paths: Option<String>,
  pub(super) deleted:      Option<i64>,
  pub(super) restarted:    Option<i64>,
}

pub(super) struct AgentView {
  pub(super) machine_id:       Uuid,
  pub(super) name:             String,
  pub(super) hostname:         String,
  pub(super) systems:          String,
  pub(super) max_jobs:         i32,
  pub(super) current_jobs:     i32,
  pub(super) connected:        bool,
  pub(super) builds_succeeded: i64,
  pub(super) builds_failed:    i64,
  pub(super) last_seen:        Option<Timestamp>,
}

pub(super) struct NotificationTaskView {
  pub(super) id:                Uuid,
  pub(super) notification_type: String,
  pub(super) status:            String,
  pub(super) attempts:          i32,
  pub(super) max_attempts:      i32,
  pub(super) next_retry_at:     Timestamp,
  pub(super) last_error:        String,
  pub(super) created_at:        Timestamp,
}

pub(super) struct PinnedOutputView {
  pub(super) build_id:           Uuid,
  pub(super) job_name:           String,
  pub(super) system:             String,
  pub(super) status:             String,
  pub(super) product_name:       String,
  pub(super) path:               String,
  pub(super) gc_root_path:       String,
  pub(super) product_created_at: Timestamp,
}

/// Everything `/admin` shows.
pub(super) struct AdminOverview {
  pub(super) status:                  SystemStatus,
  pub(super) agents:                  Vec<AgentView>,
  pub(super) agent_sort_headers:      Vec<SortHeaderView>,
  pub(super) api_keys:                Vec<ApiKeyView>,
  pub(super) notification_tasks:      Vec<NotificationTaskView>,
  pub(super) pinned_outputs:          Vec<PinnedOutputView>,
  pub(super) config_contents:         String,
  pub(super) config_editable:         bool,
  pub(super) config_read_only_reason: String,
  pub(super) gc_enabled:              bool,
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
          next_dir.as_str(),
        ),
        default_dir: sort.default_direction().as_str().to_string(),
        active,
        indicator: if active {
          active_dir.as_str().to_string()
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
    AgentSort::LastSeen => a.last_seen.cmp(&b.last_seen),
  };

  apply_direction(primary, dir)
    .then_with(|| {
      match sort {
        AgentSort::Status => {
          apply_direction(a.last_seen.cmp(&b.last_seen), SortDirection::Desc)
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

/// Load the admin overview: system status counters, builder load and
/// last-activity, API keys, queued notification tasks, pinned build outputs,
/// and the on-disk config editor when writes are enabled.
pub(super) async fn load_overview(
  state: &AppState,
  params: &AdminParams,
) -> AdminOverview {
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
    channels_count:    channels,
  };
  // Fetch connected agents
  let raw_sessions = circus_common::repo::builder_sessions::list(pool)
    .await
    .unwrap_or_default();
  let agent_sort = AgentSort::from_param(params.agent_sort.as_deref())
    .unwrap_or(AgentSort::Name);
  let agent_dir = params
    .agent_dir
    .unwrap_or_else(|| agent_sort.default_direction());
  let mut agents = raw_sessions
    .into_iter()
    .map(|s| {
      AgentView {
        machine_id:       s.machine_id,
        name:             s.name,
        hostname:         s.hostname,
        systems:          s.systems.join(", "),
        max_jobs:         s.max_jobs,
        current_jobs:     s.current_jobs,
        connected:        s.connected,
        builds_succeeded: s.builds_succeeded,
        builds_failed:    s.builds_failed,
        last_seen:        s.last_seen,
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
        created_at:   k.created_at,
        last_used_at: k.last_used_at,
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
          next_retry_at:     task.next_retry_at,
          last_error:        task.last_error.unwrap_or_default(),
          created_at:        task.created_at,
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
          job_name:           product.job_name,
          system:             product.system,
          status:             product.status.to_string(),
          product_name:       product.product_name,
          path:               product.path,
          gc_root_path:       product.gc_root_path.unwrap_or_default(),
          product_created_at: product.product_created_at,
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

  AdminOverview {
    status,
    agents,
    agent_sort_headers,
    api_keys,
    notification_tasks,
    pinned_outputs,
    config_contents,
    config_editable,
    config_read_only_reason,
    gc_enabled: state.config.gc.enabled,
  }
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
    return e.into_response();
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

/// Delete an API key from the admin page.
pub(super) async fn api_key_delete(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  ctx: DashboardContext,
  extensions: axum::http::Extensions,
  Form(form): Form<CsrfOnlyForm>,
) -> Response {
  if !ctx.is_admin {
    return StatusCode::FORBIDDEN.into_response();
  }
  if let Err(e) = ctx.check_csrf(&form.csrf_token) {
    return e.into_response();
  }
  if let Err(e) = circus_common::repo::api_keys::delete(&state.pool, id).await {
    tracing::error!(%id, "failed to delete API key: {e}");
    return Redirect::to("/admin?key=error").into_response();
  }
  crate::audit::record_action(
    &state.pool,
    &extensions,
    "API_KEY_DELETE",
    Some("api_key"),
    Some(&id.to_string()),
    serde_json::Value::Null,
  )
  .await;
  Redirect::to("/admin?key=deleted").into_response()
}

/// Requeue a failed notification task.
pub(super) async fn notification_task_retry(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  ctx: DashboardContext,
  extensions: axum::http::Extensions,
  Form(form): Form<CsrfOnlyForm>,
) -> Response {
  if !ctx.is_admin {
    return StatusCode::FORBIDDEN.into_response();
  }
  if let Err(e) = ctx.check_csrf(&form.csrf_token) {
    return e.into_response();
  }
  if let Err(e) =
    circus_common::repo::notification_tasks::requeue_failed(&state.pool, id)
      .await
  {
    tracing::error!(%id, "failed to retry notification task: {e}");
    return Redirect::to("/admin?task=error").into_response();
  }
  crate::audit::record_action(
    &state.pool,
    &extensions,
    "NOTIFICATION_TASK_RETRY",
    Some("notification_task"),
    Some(&id.to_string()),
    serde_json::Value::Null,
  )
  .await;
  Redirect::to("/admin?task=retried").into_response()
}

/// Drop a build's keep flag so its outputs can age out of GC.
pub(super) async fn build_unpin(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  ctx: DashboardContext,
  extensions: axum::http::Extensions,
  Form(form): Form<CsrfOnlyForm>,
) -> Response {
  if !ctx.is_admin {
    return StatusCode::FORBIDDEN.into_response();
  }
  if let Err(e) = ctx.check_csrf(&form.csrf_token) {
    return e.into_response();
  }
  match circus_common::repo::builds::set_keep(&state.pool, id, false).await {
    Ok(build) => {
      crate::audit::record_action(
        &state.pool,
        &extensions,
        "BUILD_UNPIN",
        Some("build"),
        Some(&id.to_string()),
        serde_json::json!({ "job_name": &build.job_name }),
      )
      .await;
      Redirect::to("/admin?unpinned=ok").into_response()
    },
    Err(e) => {
      tracing::error!(%id, "failed to unpin build: {e}");
      Redirect::to("/admin?unpinned=error").into_response()
    },
  }
}

/// Clear the failed-paths cache and requeue the cached failures it held.
pub(super) async fn failed_paths_clear(
  State(state): State<AppState>,
  ctx: DashboardContext,
  extensions: axum::http::Extensions,
  Form(form): Form<CsrfOnlyForm>,
) -> Response {
  if !ctx.is_admin {
    return StatusCode::FORBIDDEN.into_response();
  }
  if let Err(e) = ctx.check_csrf(&form.csrf_token) {
    return e.into_response();
  }
  match circus_common::repo::failed_paths_cache::clear_all(&state.pool).await {
    Ok(result) => {
      crate::audit::record_action(
        &state.pool,
        &extensions,
        "FAILED_PATHS_CACHE_CLEAR",
        Some("failed_paths_cache"),
        None,
        serde_json::json!({
          "deleted": result.deleted,
          "restarted": result.restarted,
        }),
      )
      .await;
      Redirect::to(&format!(
        "/admin?failed_paths=cleared&deleted={}&restarted={}",
        result.deleted, result.restarted
      ))
      .into_response()
    },
    Err(e) => {
      tracing::error!("failed to clear failed paths cache: {e}");
      Redirect::to("/admin?failed_paths=error").into_response()
    },
  }
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
    return e.into_response();
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
    Some(jiff::Timestamp::now() - jiff::SignedDuration::from_hours(24 * days))
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
            jiff::Timestamp::now(),
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

#[derive(serde::Deserialize)]
pub(super) struct UserCreateForm {
  username:   String,
  email:      String,
  #[serde(default)]
  full_name:  String,
  password:   String,
  role:       String,
  csrf_token: String,
}

#[derive(serde::Deserialize)]
pub(super) struct UserEnabledForm {
  enabled:    bool,
  csrf_token: String,
}

fn users_redirect(outcome: Result<UserDone, UserError>) -> Response {
  let query = match outcome {
    Ok(done) => format!("done={}", done.code()),
    Err(error) => format!("error={}", error.code()),
  };
  Redirect::to(&format!("/users?{query}")).into_response()
}

const fn user_error(error: &circus_common::CiError) -> UserError {
  match error {
    circus_common::CiError::Conflict(_) => UserError::Exists,
    circus_common::CiError::NotFound(_) => UserError::NotFound,
    _ => UserError::Failed,
  }
}

pub(super) async fn user_create(
  State(state): State<AppState>,
  ctx: DashboardContext,
  extensions: axum::http::Extensions,
  Form(form): Form<UserCreateForm>,
) -> Response {
  if !ctx.is_admin {
    return StatusCode::FORBIDDEN.into_response();
  }
  if let Err(e) = ctx.check_csrf(&form.csrf_token) {
    return e.into_response();
  }

  let Ok(role) = form.role.parse::<GlobalRole>() else {
    return users_redirect(Err(UserError::Role));
  };
  let full_name =
    Some(form.full_name.trim().to_owned()).filter(|name| !name.is_empty());
  let checks = validate_username(&form.username)
    .and_then(|()| validate_email(&form.email, state.email_regex.as_deref()))
    .and_then(|()| validate_password(&form.password))
    .and_then(|()| full_name.as_deref().map_or(Ok(()), validate_full_name));

  if let Err(error) = checks {
    return users_redirect(Err(UserError::for_field(&error.field)));
  }

  let data = CreateUser {
    username: form.username,
    email: form.email,
    full_name,
    password: form.password,
    role: Some(role),
  };

  match circus_common::repo::users::create(
    &state.pool,
    &data,
    state.email_regex.as_deref(),
  )
  .await
  {
    Ok(user) => {
      crate::audit::record_action(
        &state.pool,
        &extensions,
        "USER_CREATE",
        Some("user"),
        Some(&user.id.to_string()),
        serde_json::json!({ "username": user.username, "role": user.role }),
      )
      .await;
      users_redirect(Ok(UserDone::Created))
    },
    Err(error) => {
      tracing::warn!("dashboard user create failed: {error}");
      users_redirect(Err(user_error(&error)))
    },
  }
}

pub(super) async fn user_enabled(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  ctx: DashboardContext,
  extensions: axum::http::Extensions,
  Form(form): Form<UserEnabledForm>,
) -> Response {
  if !ctx.is_admin {
    return StatusCode::FORBIDDEN.into_response();
  }
  if let Err(e) = ctx.check_csrf(&form.csrf_token) {
    return e.into_response();
  }

  let data = UpdateUser {
    email:            None,
    full_name:        None,
    password:         None,
    role:             None,
    enabled:          Some(form.enabled),
    public_dashboard: None,
  };

  match circus_common::repo::users::update(
    &state.pool,
    id,
    &data,
    state.email_regex.as_deref(),
  )
  .await
  {
    Ok(user) => {
      crate::audit::record_action(
        &state.pool,
        &extensions,
        "USER_UPDATE",
        Some("user"),
        Some(&user.id.to_string()),
        serde_json::json!({ "fields_changed": ["enabled"], "new_role": null }),
      )
      .await;
      users_redirect(Ok(if form.enabled {
        UserDone::Enabled
      } else {
        UserDone::Disabled
      }))
    },
    Err(error) => {
      tracing::warn!(user_id = %id, "dashboard user update failed: {error}");
      users_redirect(Err(user_error(&error)))
    },
  }
}

pub(super) async fn user_delete(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  ctx: DashboardContext,
  extensions: axum::http::Extensions,
  Form(form): Form<CsrfOnlyForm>,
) -> Response {
  if !ctx.is_admin {
    return StatusCode::FORBIDDEN.into_response();
  }
  if let Err(e) = ctx.check_csrf(&form.csrf_token) {
    return e.into_response();
  }

  match circus_common::repo::users::delete(&state.pool, id).await {
    Ok(()) => {
      crate::audit::record_action(
        &state.pool,
        &extensions,
        "USER_DELETE",
        Some("user"),
        Some(&id.to_string()),
        serde_json::Value::Null,
      )
      .await;
      users_redirect(Ok(UserDone::Deleted))
    },
    Err(error) => {
      tracing::warn!(user_id = %id, "dashboard user delete failed: {error}");
      users_redirect(Err(user_error(&error)))
    },
  }
}

/// Removes a provider linked to someone else's account, for cleaning up after
/// a mistaken link or a departed provider.
pub(super) async fn user_unlink(
  State(state): State<AppState>,
  Path((id, provider)): Path<(Uuid, String)>,
  ctx: DashboardContext,
  extensions: axum::http::Extensions,
  Form(form): Form<CsrfOnlyForm>,
) -> Response {
  if !ctx.is_admin {
    return StatusCode::FORBIDDEN.into_response();
  }
  if let Err(e) = ctx.check_csrf(&form.csrf_token) {
    return e.into_response();
  }

  match circus_common::repo::users::unlink_identity(&state.pool, id, &provider)
    .await
  {
    Ok(true) => {
      crate::audit::record_action(
        &state.pool,
        &extensions,
        "OIDC_UNLINK",
        Some("user"),
        Some(&id.to_string()),
        serde_json::json!({ "provider": provider }),
      )
      .await;
    },
    Ok(false) => {},
    Err(e) => {
      tracing::error!(user_id = %id, %provider, "failed to unlink identity: {e}");
      return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    },
  }

  Redirect::to("/users").into_response()
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
    return e.into_response();
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
    return e.into_response();
  }
  if let Err(e) = circus_common::repo::news::delete(&state.pool, id).await {
    tracing::warn!(id = %id, "Failed to delete news item: {e}");
  }
  Redirect::to("/news").into_response()
}

/// `POST /starred/{id}/delete` removes a job from the viewer's own stars.
pub(super) async fn starred_delete(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
  ctx: DashboardContext,
  Form(form): Form<CsrfOnlyForm>,
) -> Response {
  let Some(user_id) = ctx.viewer_user_id else {
    return StatusCode::UNAUTHORIZED.into_response();
  };
  if let Err(e) = ctx.check_csrf(&form.csrf_token) {
    return e.into_response();
  }
  if let Err(e) =
    circus_common::repo::starred_jobs::delete_for_user(&state.pool, user_id, id)
      .await
  {
    tracing::warn!(id = %id, "Failed to unstar job: {e}");
  }
  Redirect::to("/starred").into_response()
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
    .filter(|t| {
      // Browsers read `/\host` as `//host` and drop tabs and newlines.
      t.starts_with('/')
        && !matches!(t.as_bytes().get(1), Some(b'/' | b'\\'))
        && !t.chars().any(char::is_control)
    })
    .unwrap_or(fallback)
}

pub(super) async fn jobset_delete(
  State(state): State<AppState>,
  Path(jobset_id): Path<Uuid>,
  ctx: DashboardContext,
  Form(form): Form<CsrfOnlyForm>,
) -> Result<Redirect, PageError> {
  if !ctx.is_admin {
    return Err(PageError::new((StatusCode::FORBIDDEN, "Admin required")));
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
) -> Result<Redirect, PageError> {
  if !ctx.is_admin {
    return Err(PageError::new((StatusCode::FORBIDDEN, "Admin required")));
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
) -> Result<Redirect, PageError> {
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
) -> Result<Redirect, PageError> {
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

pub(super) async fn notifications_create(
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  ctx: DashboardContext,
  Form(form): Form<NotificationCreateForm>,
) -> Result<Redirect, PageError> {
  if !ctx.is_admin {
    return Err(PageError::new((StatusCode::FORBIDDEN, "Admin required")));
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
    return Err(PageError::new((
      StatusCode::BAD_REQUEST,
      "Config must be a JSON object",
    )));
  }
  let notification_type = form
    .notification_type
    .parse::<NotificationType>()
    .map_err(|_| {
      (StatusCode::BAD_REQUEST, "Unknown notification type").into_response()
    })?;
  if !NotificationType::all().contains(&notification_type) {
    return Err(PageError::new((
      StatusCode::BAD_REQUEST,
      "Unknown notification type",
    )));
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
) -> Result<Redirect, PageError> {
  if !ctx.is_admin {
    return Err(PageError::new((StatusCode::FORBIDDEN, "Admin required")));
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
) -> Result<Redirect, PageError> {
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
    return Err(PageError::new((
      StatusCode::NOT_FOUND,
      "Build not found or no longer pending",
    )));
  }
  Ok(Redirect::to("/queue"))
}

#[derive(serde::Deserialize)]
pub(super) struct JobsetCreateForm {
  csrf_token:        String,
  name:              String,
  nix_expression:    String,
  flake_mode:        Option<String>,
  trigger_mode:      circus_common::models::JobsetTriggerMode,
  only_build_latest: Option<String>,
  #[serde(default)]
  path_filters:      String,
}

pub(super) async fn jobset_create(
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  ctx: DashboardContext,
  Form(form): Form<JobsetCreateForm>,
) -> Result<Redirect, PageError> {
  ctx
    .require_permission(Permission::CreateProjects)
    .map_err(|s| (s, "Insufficient permissions").into_response())?;
  ctx.check_csrf(&form.csrf_token)?;
  crate::routes::declarative::require_project_mutable(&state, project_id)
    .await
    .map_err(IntoResponse::into_response)?;

  let input = circus_common::models::CreateJobset {
    project_id,
    name: form.name,
    nix_expression: form.nix_expression,
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
  let failure = |message: String| {
    let message: String =
      url::form_urlencoded::byte_serialize(message.as_bytes()).collect();
    Redirect::to(&format!("/project/{project_id}?jobset_error={message}"))
  };

  if let Err(message) = circus_common::validate::Validate::validate(&input) {
    return Ok(failure(message));
  }

  match circus_common::repo::jobsets::create(&state.pool, input).await {
    Ok(_) => Ok(Redirect::to(&format!("/project/{project_id}"))),
    Err(e) => Ok(failure(e.to_string())),
  }
}

pub(super) async fn project_delete(
  State(state): State<AppState>,
  Path(project_id): Path<Uuid>,
  ctx: DashboardContext,
  extensions: axum::http::Extensions,
  Form(form): Form<CsrfOnlyForm>,
) -> Result<Redirect, PageError> {
  if !ctx.is_admin {
    return Err(PageError::new((StatusCode::FORBIDDEN, "Admin required")));
  }
  ctx.check_csrf(&form.csrf_token)?;
  let project =
    crate::routes::declarative::require_project_mutable(&state, project_id)
      .await
      .map_err(IntoResponse::into_response)?;
  circus_common::repo::projects::delete(&state.pool, project_id)
    .await
    .map_err(|e| {
      (StatusCode::BAD_REQUEST, format!("Delete failed: {e}")).into_response()
    })?;
  crate::audit::record_action(
    &state.pool,
    &extensions,
    "PROJECT_DELETE",
    Some("project"),
    Some(&project_id.to_string()),
    serde_json::json!({ "name": project.name }),
  )
  .await;
  Ok(Redirect::to("/projects"))
}
