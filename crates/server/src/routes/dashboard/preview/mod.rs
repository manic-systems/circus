//! Fixture-backed dashboard preview routes for `cargo xtask preview-frontend`.

use std::path::PathBuf;

use askama::Template;
use axum::{
  Router,
  body::Body,
  http::{StatusCode, header},
  response::{Html, IntoResponse, Redirect, Response},
  routing::{delete, get, post},
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
    .route(
      "/api/v1/projects",
      get(api::api_projects).post(api::api_project_create),
    )
    .route("/api/v1/projects/probe", post(api::api_project_probe))
    .route("/api/v1/projects/setup", post(api::api_project_setup))
    .route("/api/v1/projects/{id}", delete(api::api_ok))
    .route(
      "/api/v1/projects/{id}/jobsets",
      post(api::api_project_jobset_create),
    )
    .route("/api/v1/me/starred-jobs/{id}", delete(api::api_ok))
    .route("/api/v1/api-keys", post(api::api_key_create))
    .route("/api/v1/api-keys/{id}", delete(api::api_ok))
    .route(
      "/api/v1/builds/{build_id}/products/{product_id}/download",
      get(product_download),
    )
    .route("/logout", post(preview_logout_action))
    .route("/jobset/{id}/delete", post(preview_project_action))
    .route(
      "/evaluation/{id}/visibility",
      post(preview_evaluations_action),
    )
    .route("/evaluation/{id}/cancel", post(preview_evaluations_action))
    .route("/evaluation/{id}/restart", post(preview_evaluations_action))
    .route("/build/{id}/bump", post(preview_queue_action))
    .route("/private", get(pages::private))
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
  Redirect::temporary("/private")
}

async fn preview_logout_action() -> Redirect {
  Redirect::to("/")
}

async fn preview_project_action() -> Redirect {
  Redirect::to("/project/00000000-0000-0000-0000-000000000001")
}

async fn preview_evaluations_action() -> Redirect {
  Redirect::to("/evaluations")
}

async fn preview_queue_action() -> Redirect {
  Redirect::to("/queue")
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
