use std::{path::PathBuf, time::Duration};

use axum::{
  Router,
  extract::{Path, State},
  http::{Extensions, StatusCode},
  response::{
    IntoResponse,
    Response,
    Sse,
    sse::{Event, KeepAlive},
  },
  routing::get,
};
use circus_common::{PgPool, models::BuildStatus};
use futures::StreamExt;
use tokio::{
  io::{AsyncBufReadExt, BufReader},
  sync::mpsc::{Sender, error::SendError},
};
use uuid::Uuid;

use crate::{
  error::ApiError,
  permissions::{self, Permission},
  state::AppState,
};

async fn get_visible_build(
  state: &AppState,
  id: Uuid,
  extensions: &Extensions,
) -> Result<circus_common::Build, ApiError> {
  let build = circus_common::repo::builds::get(&state.pool, id).await?;
  circus_common::repo::evaluations::get_visible(
    &state.pool,
    build.evaluation_id,
    permissions::check(extensions, Permission::Admin),
  )
  .await?;
  Ok(build)
}

async fn get_build_log(
  extensions: Extensions,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<Response, ApiError> {
  get_visible_build(&state, id, &extensions).await?;

  let log_storage = circus_common::log_storage::LogStorage::new(
    state.config.logs.log_dir.clone(),
  )
  .map_err(|e| ApiError(circus_common::CiError::Io(e)))?;

  let path = log_storage.log_path(&id);
  let Some(path) =
    crate::routes::canonical_log_file(&state.config.logs.log_dir, &path).await
  else {
    return Ok(
      (StatusCode::NOT_FOUND, "No log available for this build")
        .into_response(),
    );
  };

  match tokio::fs::read_to_string(path).await {
    Ok(content) => {
      Ok(
        (
          StatusCode::OK,
          [("content-type", "text/plain; charset=utf-8")],
          content,
        )
          .into_response(),
      )
    },
    Err(e) => Err(ApiError(circus_common::CiError::Io(e))),
  }
}

async fn stream_build_log(
  extensions: Extensions,
  State(state): State<AppState>,
  Path(id): Path<Uuid>,
) -> Result<
  Sse<impl futures::Stream<Item = Result<Event, std::convert::Infallible>>>,
  ApiError,
> {
  let build = get_visible_build(&state, id, &extensions).await?;

  let log_storage = circus_common::log_storage::LogStorage::new(
    state.config.logs.log_dir.clone(),
  )
  .map_err(|e| ApiError(circus_common::CiError::Io(e)))?;

  let active_path = log_storage.log_path_for_active(&id);
  let final_path = log_storage.log_path(&id);
  let (tx, mut rx) = tokio::sync::mpsc::channel(16);
  tokio::spawn(follow_log(
    tx,
    active_path,
    final_path,
    state.config.logs.log_dir.clone(),
    state.pool.clone(),
    build.id,
  ));
  let stream = futures::stream::poll_fn(move |cx| rx.poll_recv(cx)).map(Ok);

  Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// Sends end once the client disconnects, the build finishes, or the log
/// cannot be read.
async fn follow_log(
  tx: Sender<Event>,
  active_path: PathBuf,
  final_path: PathBuf,
  log_dir: PathBuf,
  pool: PgPool,
  build_id: Uuid,
) -> Result<(), SendError<Event>> {
  if !active_path.exists() && !final_path.exists() {
    let mut found = false;
    for _ in 0..30 {
      tokio::time::sleep(Duration::from_secs(1)).await;
      if active_path.exists() || final_path.exists() {
        found = true;
        break;
      }
    }
    if !found {
      return tx
        .send(Event::default().data("No log file available"))
        .await;
    }
  }

  let path = if active_path.exists() {
    active_path
  } else {
    final_path
  };
  let Some(path) = crate::routes::canonical_log_file(&log_dir, &path).await
  else {
    return tx
      .send(Event::default().data("Log file is unavailable"))
      .await;
  };
  let Ok(file) = tokio::fs::File::open(&path).await else {
    return tx
      .send(Event::default().data("Failed to open log file"))
      .await;
  };

  let mut reader = BufReader::new(file);
  let mut line = String::new();
  let mut consecutive_empty = 0u32;

  loop {
    line.clear();
    match reader.read_line(&mut line).await {
      Ok(0) => {
        consecutive_empty += 1;
        if consecutive_empty > 5 {
          if let Ok(build) =
            circus_common::repo::builds::get(&pool, build_id).await
            && build.status != BuildStatus::Running
            && build.status != BuildStatus::Pending
          {
            return tx
              .send(Event::default().event("done").data("Build completed"))
              .await;
          }
          consecutive_empty = 0;
        }
        if tx.is_closed() {
          return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
      },
      Ok(_) => {
        consecutive_empty = 0;
        tx.send(Event::default().data(line.trim_end())).await?;
      },
      Err(_) => return Ok(()),
    }
  }
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/builds/{id}/log", get(get_build_log))
    .route("/builds/{id}/log/stream", get(stream_build_log))
}
