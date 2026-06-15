use axum::{Json, Router, extract::State, http::Extensions, routing::get};

use crate::{operator, permissions, state::AppState};

async fn overview(
  State(state): State<AppState>,
  extensions: Extensions,
) -> Json<operator::OperatorOverview> {
  Json(
    operator::overview(
      &state,
      permissions::check(&extensions, permissions::Permission::Admin),
    )
    .await,
  )
}

async fn failures(
  State(state): State<AppState>,
) -> Json<Vec<operator::OperatorBuild>> {
  Json(operator::failures(&state).await)
}

async fn recent_builds(
  State(state): State<AppState>,
) -> Json<Vec<operator::OperatorBuild>> {
  Json(operator::recent_builds(&state).await)
}

async fn projects(
  State(state): State<AppState>,
  extensions: Extensions,
) -> Json<Vec<operator::OperatorProject>> {
  Json(
    operator::projects(
      &state,
      permissions::check(&extensions, permissions::Permission::Admin),
    )
    .await,
  )
}

async fn queue(
  State(state): State<AppState>,
) -> Json<Vec<operator::OperatorQueueSystem>> {
  Json(operator::queue(&state).await)
}

async fn workers(
  State(state): State<AppState>,
) -> Json<Vec<operator::OperatorWorker>> {
  Json(operator::worker_summary(&state).await)
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/operator/overview", get(overview))
    .route("/operator/failures", get(failures))
    .route("/operator/recent-builds", get(recent_builds))
    .route("/operator/projects", get(projects))
    .route("/operator/queue", get(queue))
    .route("/operator/workers", get(workers))
}
