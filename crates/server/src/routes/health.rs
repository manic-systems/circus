use axum::{
  Json,
  Router,
  extract::State,
  http::StatusCode,
  response::IntoResponse,
  routing::get,
};
use circus_common::service_heartbeat::{
  SERVICE_EVALUATOR,
  SERVICE_QUEUE_RUNNER,
  ServiceStatus,
  status_for,
};
use serde::Serialize;

use crate::state::AppState;

/// Heartbeats older than `STALE_THRESHOLD_MULTIPLIER * poll_interval_seconds`
/// are treated as stale. 3 covers one missed tick plus jitter without firing
/// false alarms on healthy systems.
const STALE_THRESHOLD_MULTIPLIER: u32 = 3;

#[derive(Serialize)]
struct HealthResponse {
  /// Overall status: "ok" if everything healthy, "degraded" otherwise.
  status:   &'static str,
  /// Build of the server answering this request.
  version:  &'static str,
  /// True if the database is reachable.
  database: bool,
  /// Per-service liveness, as reported by background-service heartbeats.
  services: Vec<ServiceStatus>,
}

async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
  let db_ok = circus_common::Database::health_check(&state.pool)
    .await
    .is_ok();

  // If the database is down we can't check service heartbeats either; report
  // degraded with no service detail.
  let services = if db_ok {
    status_for(
      &state.pool,
      &[SERVICE_EVALUATOR, SERVICE_QUEUE_RUNNER],
      STALE_THRESHOLD_MULTIPLIER,
    )
    .await
    .unwrap_or_default()
  } else {
    Vec::new()
  };

  let all_services_ok = services.iter().all(|s| s.healthy);
  let healthy = db_ok && all_services_ok;
  let status = if healthy { "ok" } else { "degraded" };

  let body = Json(HealthResponse {
    status,
    version: circus_common::version::long(),
    database: db_ok,
    services,
  });

  // 200 when fully healthy, 503 when any subsystem is degraded. The body is
  // returned in both cases so probes have machine-readable detail.
  let code = if healthy {
    StatusCode::OK
  } else {
    StatusCode::SERVICE_UNAVAILABLE
  };

  (code, body)
}

pub fn router() -> Router<AppState> {
  Router::new().route("/health", get(health_check))
}
