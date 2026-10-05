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
  routing::{get, post},
};

use crate::state::AppState;

mod admin;
pub mod assets;
mod auth;
mod build_log;
pub mod live;
mod pages;
mod preview;
mod shared;
pub(crate) mod templates;

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/login", get(auth::login_page).post(auth::login_action))
    .route("/logout", post(auth::logout_action))
    .route("/account", get(auth::account_page))
    .route(
      "/account/link/{provider}",
      get(auth::account_link_page).post(auth::account_link),
    )
    .route("/account/unlink/{provider}", post(auth::account_unlink))
    .route("/", get(pages::home))
    .route("/projects", get(pages::projects_page))
    .route("/projects/new", get(pages::project_setup_page))
    .route("/project/{id}", get(pages::project_page))
    .route(
      "/project/{id}/notifications",
      get(admin::notifications_page).post(admin::notifications_create),
    )
    .route(
      "/project/{id}/notifications/{config_id}/delete",
      post(admin::notifications_delete),
    )
    .route("/jobset/{id}", get(pages::jobset_page))
    .route("/jobset/{id}/jobs", get(pages::jobset_jobs_page))
    .route("/jobset/{id}/delete", post(admin::jobset_delete))
    .route("/evaluations", get(pages::evaluations_page))
    .route("/evaluation/{id}", get(pages::evaluation_page))
    .route(
      "/evaluation/{id}/visibility",
      post(admin::evaluation_visibility),
    )
    .route("/evaluation/{id}/cancel", post(admin::evaluation_cancel))
    .route("/evaluation/{id}/restart", post(admin::evaluation_restart))
    .route("/builds", get(pages::builds_page))
    .route("/build/{id}", get(pages::build_page))
    .route("/build/{id}/log", get(pages::build_log))
    .route("/build/{id}/bump", post(admin::queue_bump))
    .route("/channels", get(pages::channels_page))
    .route("/channel/{id}", get(pages::channel_page))
    .route("/news", get(admin::news_page).post(admin::news_create))
    .route("/news/{id}/delete", post(admin::news_delete))
    .route("/admin", get(admin::admin_page))
    .route("/admin/store-gc", post(admin::store_gc))
    .route("/users", get(admin::users_page))
    .route("/users/{id}/unlink/{provider}", post(admin::user_unlink))
    .route("/starred", get(pages::starred_page))
    .route("/metrics", get(pages::metrics_page))
    .route("/caches", get(pages::caches_page))
    .route("/caches/{name}", get(pages::cache_detail_page))
    .route("/caches/{name}/gc", post(admin::cache_gc))
    .route("/caches/{name}/nars", get(pages::cache_nars_page))
}

pub fn preview_router() -> Router {
  preview::router()
}
