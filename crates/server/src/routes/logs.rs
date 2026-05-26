use axum::{
  Router,
  extract::{Path, State},
  http::StatusCode,
  response::{
    IntoResponse,
    Response,
    Sse,
    sse::{Event, KeepAlive},
  },
  routing::get,
};
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

async fn get_build_log(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
  // Verify build exists
  let _build = circus_common::repo::builds::get(&state.pool, id)
    .await
    .map_err(ApiError)?;

  let log_storage = circus_common::log_storage::LogStorage::new(
    state.config.logs.log_dir.clone(),
  )
  .map_err(|e| ApiError(circus_common::CiError::Io(e)))?;

  match log_storage.read_log(&id) {
    Ok(Some(content)) => {
      Ok(
        (
          StatusCode::OK,
          [("content-type", "text/plain; charset=utf-8")],
          content,
        )
          .into_response(),
      )
    },
    Ok(None) => {
      Ok(
        (StatusCode::NOT_FOUND, "No log available for this build")
          .into_response(),
      )
    },
    Err(e) => Err(ApiError(circus_common::CiError::Io(e))),
  }
}

async fn stream_build_log(
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<
  Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>>,
  ApiError,
> {
  let build = circus_common::repo::builds::get(&state.pool, id)
    .await
    .map_err(ApiError)?;

  let log_storage = circus_common::log_storage::LogStorage::new(
    state.config.logs.log_dir.clone(),
  )
  .map_err(|e| ApiError(circus_common::CiError::Io(e)))?;

  let active_path = log_storage.log_path_for_active(&id);
  let final_path = log_storage.log_path(&id);
  let pool = state.pool.clone();
  let build_id = build.id;

  let stream = async_stream::stream! {
      use tokio::io::{AsyncBufReadExt, BufReader};

      // Determine which file to read
      let path = if active_path.exists() {
          active_path.clone()
      } else if final_path.exists() {
          final_path.clone()
      } else {
          // Wait for the file to appear
          let mut found = false;
          for _ in 0..30 {
              tokio::time::sleep(std::time::Duration::from_secs(1)).await;
              if active_path.exists() || final_path.exists() {
                  found = true;
                  break;
              }
          }
          if !found {
              yield Ok(Event::default().data("No log file available"));
              return;
          }
          if active_path.exists() { active_path.clone() } else { final_path.clone() }
      };

      let Ok(file) = tokio::fs::File::open(&path).await else {
        yield Ok(Event::default().data("Failed to open log file"));
        return;
      };

      let mut reader = BufReader::new(file);
      let mut line = String::new();
      let mut consecutive_empty = 0u32;

      loop {
          line.clear();
          match reader.read_line(&mut line).await {
              Ok(0) => {
                  // EOF - check if build is still running
                  consecutive_empty += 1;
                  if consecutive_empty > 5 {
                      // Check build status
                      if let Ok(b) = circus_common::repo::builds::get(&pool, build_id).await
                          && b.status != circus_common::models::BuildStatus::Running
                              && b.status != circus_common::models::BuildStatus::Pending {
                              yield Ok(Event::default().event("done").data("Build completed"));
                              return;
                          }
                      consecutive_empty = 0;
                  }
                  tokio::time::sleep(std::time::Duration::from_millis(500)).await;
              }
              Ok(_) => {
                  consecutive_empty = 0;
                  yield Ok(Event::default().data(line.trim_end()));
              }
              Err(_) => return,
          }
      }
  };

  Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/builds/{id}/log", get(get_build_log))
    .route("/builds/{id}/log/stream", get(stream_build_log))
}
