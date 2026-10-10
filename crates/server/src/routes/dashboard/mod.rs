//! Server-rendered dashboard. Originally one ~2000 line file; broken into
//! per-concern modules to keep maintenance focused:
//!
//! - [`shared`]: view models, formatters, badges, per-request auth helpers
//! - [`templates`]: every askama `#[derive(Template)]` struct
//! - [`auth`]: login / logout
//! - [`pages`]: read-only viewing pages (home, projects, jobsets, ...)
//! - [`admin`]: admin-only pages and the forms that mutate server state (news,
//!   project notifications, users)
//!
//! The public surface is just [`router`].

use axum::{
  Router,
  routing::{get, get_service, post, post_service},
};
use topcoat::router::tower::TowerService;

use crate::state::AppState;

mod admin;
pub mod assets;
mod auth;
mod build_log;
mod components;
mod layout;
pub mod live;
mod pages;
mod preview;
mod shared;
pub(crate) mod templates;
pub(crate) mod views;

/// Topcoat pages route their GETs to `live`, which also serves the runtime
/// under `/_topcoat`.
pub fn router(live: TowerService) -> Router<AppState> {
  Router::new()
    .route("/queue", get_service(live.clone()))
    .route("/login", get_service(live.clone()).post(auth::login_action))
    .route("/logout", post(auth::logout_action))
    .route("/account", get_service(live.clone()))
    .route(
      "/account/link/{provider}",
      get_service(live.clone()).post(auth::account_link),
    )
    .route("/account/unlink/{provider}", post(auth::account_unlink))
    .route("/", get_service(live.clone()))
    .route("/projects", get(pages::projects_page))
    .route("/projects/new", get(pages::project_setup_page))
    .route("/project/{id}", get_service(live.clone()))
    .route("/project/{id}/jobsets", post(admin::jobset_create))
    .route("/project/{id}/delete", post(admin::project_delete))
    .route(
      "/project/{id}/notifications",
      get_service(live.clone()).post(admin::notifications_create),
    )
    .route(
      "/project/{id}/notifications/{config_id}/delete",
      post(admin::notifications_delete),
    )
    .route("/jobset/{id}", get_service(live.clone()))
    .route("/jobset/{id}/jobs", get_service(live.clone()))
    .route("/jobset/{id}/delete", post(admin::jobset_delete))
    .route("/evaluations", get_service(live.clone()))
    .route("/evaluation/{id}", get_service(live.clone()))
    .route(
      "/evaluation/{id}/visibility",
      post(admin::evaluation_visibility),
    )
    .route("/evaluation/{id}/cancel", post(admin::evaluation_cancel))
    .route("/evaluation/{id}/restart", post(admin::evaluation_restart))
    .route("/builds", get_service(live.clone()))
    .route("/build/{id}", get_service(live.clone()))
    .route("/build/{id}/log", get_service(live.clone()))
    .route("/build/{id}/bump", post(admin::queue_bump))
    .route("/channels", get_service(live.clone()))
    .route("/channel/{id}", get_service(live.clone()))
    .route("/news", get_service(live.clone()).post(admin::news_create))
    .route("/news/{id}/delete", post(admin::news_delete))
    .route("/admin", get_service(live.clone()))
    .route("/admin/api-keys", post_service(live.clone()))
    .route("/admin/api-keys/{id}/delete", post(admin::api_key_delete))
    .route("/admin/config", post_service(live.clone()))
    .route(
      "/admin/notification-tasks/{id}/retry",
      post(admin::notification_task_retry),
    )
    .route("/admin/pinned-builds/{id}/unpin", post(admin::build_unpin))
    .route("/admin/failed-paths/clear", post(admin::failed_paths_clear))
    .route("/admin/store-gc", post(admin::store_gc))
    .route("/users", get_service(live.clone()).post(admin::user_create))
    .route("/users/{id}/enabled", post(admin::user_enabled))
    .route("/users/{id}/delete", post(admin::user_delete))
    .route("/users/{id}/unlink/{provider}", post(admin::user_unlink))
    .route("/starred", get_service(live.clone()))
    .route("/starred/{id}/delete", post(admin::starred_delete))
    .route("/metrics", get(pages::metrics_page))
    .route("/caches", get_service(live.clone()))
    .route("/caches/{name}", get(pages::cache_detail_page))
    .route("/caches/{name}/gc", post(admin::cache_gc))
    .route("/caches/{name}/nars", get_service(live.clone()))
    .route_service("/_topcoat/{*rest}", live)
}

pub fn preview_router() -> Router {
  preview::router()
}
