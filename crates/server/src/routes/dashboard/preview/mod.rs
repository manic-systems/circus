//! Fixture-backed dashboard preview routes for `cargo xtask preview-frontend`.

use std::path::PathBuf;

use askama::Template;
use axum::{
  Router,
  body::Body,
  http::{StatusCode, header},
  response::{Html, IntoResponse, Redirect, Response},
  routing::{get, post},
};
use tower_http::services::ServeDir;

mod api;
mod fixtures;
mod pages;

pub fn router() -> Router {
  Router::new()
    .route("/__preview", get(index))
    .route("/static/theme.css", get(theme_css))
    .nest_service("/static", ServeDir::new(static_dir()))
    .route("/api/v1/projects", get(api::api_projects))
    .route("/api/v1/projects/probe", post(api::api_project_probe))
    .route("/api/v1/projects/setup", post(api::api_project_setup))
    .route(
      "/api/v1/metrics/timeseries/builds",
      get(api::api_metrics_builds),
    )
    .route(
      "/api/v1/metrics/timeseries/duration",
      get(api::api_metrics_duration),
    )
    .route("/api/v1/metrics/systems", get(api::api_metrics_systems))
    .route(
      "/api/v1/admin/caches/{name}/storage-timeseries",
      get(api::api_cache_storage_timeseries),
    )
    .route(
      "/api/v1/admin/caches/{name}/traffic-timeseries",
      get(api::api_cache_traffic_timeseries),
    )
    .route(
      "/api/v1/builds/{build_id}/products/{product_id}/download",
      get(product_download),
    )
    .route("/", get(pages::home))
    .route("/logout", post(preview_logout_action))
    .route("/projects", get(pages::projects))
    .route("/projects/new", get(pages::project_setup))
    .route(
      "/project/{id}/notifications",
      get(pages::notifications).post(preview_notifications_action),
    )
    .route(
      "/project/{id}/notifications/{config_id}/delete",
      post(preview_notifications_action),
    )
    .route("/project/{id}", get(pages::project))
    .route("/jobset/{id}", get(pages::jobset))
    .route("/jobset/{id}/jobs", get(pages::jobset_jobs))
    .route("/jobset/{id}/delete", post(preview_project_action))
    .route("/evaluations", get(pages::evaluations))
    .route("/evaluation/{id}", get(pages::evaluation))
    .route(
      "/evaluation/{id}/visibility",
      post(preview_evaluations_action),
    )
    .route("/builds", get(pages::builds))
    .route("/build/{id}", get(pages::build))
    .route("/build/{id}/log", get(build_log))
    .route("/build/{id}/bump", post(preview_queue_action))
    .route("/queue", get(pages::queue))
    .route("/channels", get(pages::channels))
    .route("/channel/{id}", get(pages::channel))
    .route("/news", get(pages::news).post(preview_news_action))
    .route("/news/{id}/delete", post(preview_news_action))
    .route("/admin", get(pages::admin))
    .route("/users", get(pages::users))
    .route("/starred", get(pages::starred))
    .route("/metrics", get(pages::metrics))
    .route("/login", get(pages::login).post(preview_login_action))
    .route("/private", get(pages::private))
    .route("/caches", get(pages::caches))
    .route("/caches/{name}", get(pages::cache_detail))
    .route("/caches/{name}/nars", get(pages::cache_nars))
}

pub(super) fn render<T: Template>(template: T) -> Response {
  match template.render() {
    Ok(html) => Html(html).into_response(),
    Err(error) => {
      (
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("Template error: {error}"),
      )
        .into_response()
    },
  }
}

async fn index() -> Redirect {
  Redirect::temporary("/")
}

async fn preview_login_action() -> Redirect {
  Redirect::to("/login")
}

async fn preview_logout_action() -> Redirect {
  Redirect::to("/")
}

async fn preview_project_action() -> Redirect {
  Redirect::to("/project/00000000-0000-0000-0000-000000000001")
}

async fn preview_notifications_action() -> Redirect {
  Redirect::to("/project/00000000-0000-0000-0000-000000000001/notifications")
}

async fn preview_evaluations_action() -> Redirect {
  Redirect::to("/evaluations")
}

async fn preview_queue_action() -> Redirect {
  Redirect::to("/queue")
}

async fn preview_news_action() -> Redirect {
  Redirect::to("/news")
}

async fn build_log() -> Response {
  Response::builder()
    .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
    .header(header::CACHE_CONTROL, "no-cache")
    .body(Body::from(
      "preview build log\n[1/2] evaluating derivation\n[2/2] build completed\n",
    ))
    .unwrap_or_else(|error| {
      Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::from(format!("response builder failed: {error}")))
        .unwrap_or_else(|_| Response::new(Body::empty()))
    })
}

async fn product_download() -> Response {
  Response::builder()
    .header(header::CONTENT_TYPE, "application/octet-stream")
    .header(header::CACHE_CONTROL, "no-cache")
    .body(Body::from("preview artifact\n"))
    .unwrap_or_else(|error| {
      Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::from(format!("response builder failed: {error}")))
        .unwrap_or_else(|_| Response::new(Body::empty()))
    })
}

async fn theme_css() -> Response {
  Response::builder()
    .header(header::CONTENT_TYPE, "text/css")
    .header(header::CACHE_CONTROL, "no-cache")
    .body(Body::from(
      ":root {
  --accent: #111827;
  --accent-hover: #000000;
  --accent-strong: #374151;
  --accent-contrast: #ffffff;
  --accent-hover-contrast: #ffffff;
}
",
    ))
    .unwrap_or_else(|error| {
      Response::builder()
        .status(StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::from(format!("response builder failed: {error}")))
        .unwrap_or_else(|_| Response::new(Body::empty()))
    })
}

fn static_dir() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("static")
}
