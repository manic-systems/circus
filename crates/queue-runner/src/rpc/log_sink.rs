//! `LogSink` implementation: receives log chunks from one agent build and
//! appends them to the live log file for that build.
//!
//! The runner creates one of these per dispatched build, hands it to the
//! agent in `Builder.assign`, and drops it when the build finishes (or
//! when `close()` is called).

use std::{path::PathBuf, sync::Arc};

use capnp::capability::Promise;
use circus_proto::log_sink;
use tokio::{
  fs::{File, OpenOptions},
  io::AsyncWriteExt as _,
  sync::Mutex,
};

pub struct LogSinkImpl {
  inner: Arc<Inner>,
}

struct Inner {
  path: PathBuf,
  file: Mutex<Option<File>>,
}

impl LogSinkImpl {
  pub fn new(path: PathBuf) -> Self {
    Self {
      inner: Arc::new(Inner {
        path,
        file: Mutex::new(None),
      }),
    }
  }
}

async fn open(inner: &Inner) -> std::io::Result<File> {
  if let Some(parent) = inner.path.parent() {
    tokio::fs::create_dir_all(parent).await.ok();
  }
  OpenOptions::new()
    .create(true)
    .append(true)
    .open(&inner.path)
    .await
}

#[allow(refining_impl_trait_internal, refining_impl_trait_reachable)]
impl log_sink::Server for LogSinkImpl {
  fn write(
    self: capnp::capability::Rc<Self>,
    params: log_sink::WriteParams,
    _results: log_sink::WriteResults,
  ) -> Promise<(), capnp::Error> {
    let inner = Arc::clone(&self.inner);
    Promise::from_future(async move {
      let pr = params.get().map_err(capnp::Error::from)?;
      let chunk = pr.get_chunk().map_err(capnp::Error::from)?.to_vec();

      let mut guard = inner.file.lock().await;
      if guard.is_none() {
        let f = open(&inner).await.map_err(|e| {
          capnp::Error::failed(format!(
            "open log {}: {e}",
            inner.path.display()
          ))
        })?;
        *guard = Some(f);
      }
      let f = guard.as_mut().expect("just initialised");
      f.write_all(&chunk)
        .await
        .map_err(|e| capnp::Error::failed(format!("write log: {e}")))?;
      f.write_all(b"\n")
        .await
        .map_err(|e| capnp::Error::failed(format!("write log: {e}")))?;
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
      if let Some(mut f) = inner.file.lock().await.take() {
        let _ = f.flush().await;
      }
      Ok(())
    })
  }
}
