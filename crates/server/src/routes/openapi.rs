//! `OpenAPI` 3.1 description of the public REST API.
//!
//! Coverage policy: every HTTP route registered by `routes::router` has an
//! entry except dashboard HTML pages and static assets. Binary/streaming
//! endpoints are documented at the operation level even when they are not JSON.

use std::sync::LazyLock;

use axum::{
  Router,
  http::header::CONTENT_TYPE,
  response::{IntoResponse, Response},
  routing::get,
};
use serde_json::Value;

use crate::state::AppState;

/// The document as written, with a placeholder `info.version`.
pub const DOCUMENT: &str = include_str!("../../openapi.json");

static SPEC: LazyLock<Vec<u8>> = LazyLock::new(|| {
  #[expect(
    clippy::expect_used,
    reason = "the bundled document is checked by the openapi drift test"
  )]
  let mut spec: Value =
    serde_json::from_str(DOCUMENT).expect("openapi.json is valid JSON");

  if let Some(version) = spec.pointer_mut("/info/version") {
    *version = Value::from(circus_common::version::VERSION);
  }

  spec.to_string().into_bytes()
});

async fn openapi_spec() -> Response {
  ([(CONTENT_TYPE, "application/json")], SPEC.as_slice()).into_response()
}

pub fn router() -> Router<AppState> {
  Router::new().route("/api/v1/openapi.json", get(openapi_spec))
}
