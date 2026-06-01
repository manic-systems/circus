//! `LogSink` implementation: receives log chunks from one agent build and
//! appends them to the live log file for that build.
//!
//! The runner creates one of these per dispatched build, hands it to the
//! agent in `Builder.assign`, and drops it when the build finishes (or
//! when `close()` is called).

use std::{path::PathBuf, sync::Arc};

use capnp::capability::Promise;
use circus_proto::{limits, log_sink};
use tokio::{
  fs::{File, OpenOptions},
  io::AsyncWriteExt as _,
  sync::Mutex,
};

pub struct LogSinkImpl {
  inner: Arc<Inner>,
}

struct Inner {
  path:      PathBuf,
  max_bytes: u64,
  state:     Mutex<State>,
}

struct State {
  file:          Option<File>,
  bytes_written: u64,
  closed:        bool,
}

impl LogSinkImpl {
  #[must_use]
  pub fn new(path: PathBuf, max_bytes: u64) -> Self {
    Self {
      inner: Arc::new(Inner {
        path,
        max_bytes,
        state: Mutex::new(State {
          file:          None,
          bytes_written: 0,
          closed:        false,
        }),
      }),
    }
  }
}

async fn open(inner: &Inner) -> std::io::Result<File> {
  if let Some(parent) = inner.path.parent() {
    let _ = tokio::fs::create_dir_all(parent).await;
  }
  OpenOptions::new()
    .create(true)
    .append(true)
    .open(&inner.path)
    .await
}

#[allow(refining_impl_trait_internal, refining_impl_trait_reachable)]
impl log_sink::Server for LogSinkImpl {
  #[expect(
    clippy::significant_drop_tightening,
    reason = "file lock held during writes"
  )]
  fn write(
    self: capnp::capability::Rc<Self>,
    params: log_sink::WriteParams,
    _results: log_sink::WriteResults,
  ) -> Promise<(), capnp::Error> {
    let inner = Arc::clone(&self.inner);
    Promise::from_future(async move {
      let pr = params.get()?;
      let chunk = pr.get_chunk()?.to_vec();
      if chunk.len() > limits::MAX_LOG_CHUNK_BYTES {
        return Err(capnp::Error::failed(format!(
          "log chunk too large: {} > {}",
          chunk.len(),
          limits::MAX_LOG_CHUNK_BYTES
        )));
      }

      let accounted = chunk.len() as u64 + 1;
      let mut state = inner.state.lock().await;
      if state.closed {
        return Err(capnp::Error::failed("log sink is closed".into()));
      }
      if state.bytes_written.saturating_add(accounted) > inner.max_bytes {
        return Err(capnp::Error::failed(format!(
          "log size limit exceeded: {} > {}",
          state.bytes_written.saturating_add(accounted),
          inner.max_bytes
        )));
      }
      if state.file.is_none() {
        let f = open(&inner).await.map_err(|e| {
          capnp::Error::failed(format!(
            "open log {}: {e}",
            inner.path.display()
          ))
        })?;
        state.file = Some(f);
      }
      if let Some(f) = state.file.as_mut() {
        f.write_all(&chunk)
          .await
          .map_err(|e| capnp::Error::failed(format!("write log: {e}")))?;
        f.write_all(b"\n")
          .await
          .map_err(|e| capnp::Error::failed(format!("write log: {e}")))?;
        state.bytes_written = state.bytes_written.saturating_add(accounted);
      }
      Ok(())
    })
  }

  fn close(
    self: capnp::capability::Rc<Self>,
    _params: log_sink::CloseParams,
    _results: log_sink::CloseResults,
  ) -> Promise<(), capnp::Error> {
    let inner = Arc::clone(&self.inner);
    Promise::from_future(async move {
      let mut state = inner.state.lock().await;
      state.closed = true;
      let file_opt = state.file.take();
      drop(state);
      if let Some(mut f) = file_opt {
        let _ = f.flush().await;
      }
      Ok(())
    })
  }
}
