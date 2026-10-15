use std::{path::PathBuf, time::Duration};

use async_compression::tokio::bufread::ZstdDecoder;
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
use circus_common::{
  CiError,
  PgPool,
  log_storage::{LogStorage, read_log_file},
  models::BuildStatus,
};
use futures::StreamExt;
use tokio::{
  io::{AsyncBufRead, AsyncBufReadExt, BufReader},
  sync::mpsc::{Sender, error::SendError},
  time::sleep,
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

  let log_storage = LogStorage::new(
    state.config.logs.log_dir.clone(),
    state.config.logs.compress,
  )
  .map_err(|e| ApiError(CiError::Io(e)))?;

  let Some(path) = log_storage
    .find_log_path(&id)
    .await
    .map_err(|e| ApiError(CiError::Io(e)))?
  else {
    return Ok(
      (StatusCode::NOT_FOUND, "No log available for this build")
        .into_response(),
    );
  };
  let Some(path) =
    crate::routes::canonical_log_file(&state.config.logs.log_dir, &path).await
  else {
    return Ok(
      (StatusCode::NOT_FOUND, "No log available for this build")
        .into_response(),
    );
  };

  match read_log_file(&path).await {
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
    Err(e) => Err(ApiError(CiError::Io(e))),
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

  let log_storage = LogStorage::new(
    state.config.logs.log_dir.clone(),
    state.config.logs.compress,
  )
  .map_err(|e| ApiError(CiError::Io(e)))?;

  let (tx, mut rx) = tokio::sync::mpsc::channel(16);
  tokio::spawn(follow_log(
    tx,
    log_storage,
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
  log_storage: LogStorage,
  log_dir: PathBuf,
  pool: PgPool,
  build_id: Uuid,
) -> Result<(), SendError<Event>> {
  let active_path = log_storage.log_path_for_active(&build_id);
  let mut path = None;
  for attempt in 0..=30 {
    if active_path.exists() {
      path = Some(active_path.clone());
      break;
    }
    match log_storage.find_log_path(&build_id).await {
      Ok(Some(final_path)) => {
        path = Some(final_path);
        break;
      },
      Ok(None) => {},
      Err(error) => {
        tracing::warn!(%build_id, %error, "Failed to locate build log");
        break;
      },
    }
    if attempt < 30 {
      sleep(Duration::from_secs(1)).await;
    }
  }
  let Some(path) = path else {
    return tx
      .send(Event::default().data("No log file available"))
      .await;
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

  let reader = BufReader::new(file);
  if path.extension().is_some_and(|extension| extension == "zst") {
    follow_reader(tx, BufReader::new(ZstdDecoder::new(reader)), pool, build_id)
      .await
  } else {
    follow_reader(tx, reader, pool, build_id).await
  }
}

async fn follow_reader(
  tx: Sender<Event>,
  mut reader: impl AsyncBufRead + Unpin,
  pool: PgPool,
  build_id: Uuid,
) -> Result<(), SendError<Event>> {
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
