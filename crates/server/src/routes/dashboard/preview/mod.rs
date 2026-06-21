//! Fixture-backed dashboard preview routes for `cargo xtask preview-frontend`.

use std::path::PathBuf;

use askama::Template;
use axum::{
  Router,
  body::Body,
  http::{StatusCode, header},
  response::{Html, IntoResponse, Redirect, Response},
  routing::get,
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
    .route("/", get(pages::home))
    .route("/projects", get(pages::projects))
    .route("/projects/new", get(pages::project_setup))
    .route("/project/{id}", get(pages::project))
    .route("/jobset/{id}", get(pages::jobset))
    .route("/jobset/{id}/jobs", get(pages::jobset_jobs))
    .route("/evaluations", get(pages::evaluations))
    .route("/evaluation/{id}", get(pages::evaluation))
    .route("/builds", get(pages::builds))
    .route("/build/{id}", get(pages::build))
    .route("/queue", get(pages::queue))
    .route("/channels", get(pages::channels))
    .route("/channel/{id}", get(pages::channel))
    .route("/news", get(pages::news))
    .route("/admin", get(pages::admin))
    .route("/users", get(pages::users))
    .route("/starred", get(pages::starred))
    .route("/metrics", get(pages::metrics))
    .route("/login", get(pages::login))
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

async fn theme_css() -> Response {
  Response::builder()
    .header(header::CONTENT_TYPE, "text/css")
    .header(header::CACHE_CONTROL, "no-cache")
    .body(Body::from(
      ":root {
  --accent: #111827;
  --accent-hover: #000000;
  --accent-strong: #374151;
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
