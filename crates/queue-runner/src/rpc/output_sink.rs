//! `OutputSink` pipes a build's output closure into `nix-store --import` on the
//! runner. `close` resolves once the import child exits, so the agent can await
//! it before reporting.

use std::{process::Stdio, sync::Arc};

use capnp::capability::Promise;
use circus_proto::{limits, output_sink};
use tokio::{
  io::AsyncWriteExt as _,
  process::{Child, ChildStdin, Command},
  sync::Mutex,
};

pub struct OutputSinkImpl {
  inner: Arc<Inner>,
}

struct Inner {
  build_id: String,
  state:    Mutex<State>,
}

struct State {
  child:  Option<Child>,
  stdin:  Option<ChildStdin>,
  closed: bool,
}

impl OutputSinkImpl {
  #[must_use]
  pub fn new(build_id: String) -> Self {
    Self {
      inner: Arc::new(Inner {
        build_id,
        state: Mutex::new(State {
          child:  None,
          stdin:  None,
          closed: false,
        }),
      }),
    }
  }
}

fn spawn_import() -> std::io::Result<(Child, ChildStdin)> {
  let mut child = Command::new("nix-store")
    .arg("--import")
    .stdin(Stdio::piped())
    .stdout(Stdio::null())
    .stderr(Stdio::inherit())
    .spawn()?;
  let stdin = child.stdin.take().ok_or_else(|| {
    std::io::Error::other("nix-store --import produced no stdin")
  })?;
  Ok((child, stdin))
}

#[allow(refining_impl_trait_internal, refining_impl_trait_reachable)]
impl output_sink::Server for OutputSinkImpl {
  #[expect(
    clippy::significant_drop_tightening,
    reason = "import child lock held across the chunk write"
  )]
  fn write(
    self: capnp::capability::Rc<Self>,
    params: output_sink::WriteParams,
    _results: output_sink::WriteResults,
  ) -> Promise<(), capnp::Error> {
    let inner = Arc::clone(&self.inner);
    Promise::from_future(async move {
      let pr = params.get()?;
      let chunk = pr.get_chunk()?.to_vec();
      if chunk.len() > limits::MAX_NAR_CHUNK_BYTES {
        return Err(capnp::Error::failed(format!(
          "output chunk too large: {} > {}",
          chunk.len(),
          limits::MAX_NAR_CHUNK_BYTES
        )));
      }

      let mut state = inner.state.lock().await;
      if state.closed {
        return Err(capnp::Error::failed("output sink is closed".into()));
      }
      if state.child.is_none() {
        let (child, stdin) = spawn_import().map_err(|e| {
          capnp::Error::failed(format!("spawn nix-store --import: {e}"))
        })?;
        state.child = Some(child);
        state.stdin = Some(stdin);
      }
      if let Some(stdin) = state.stdin.as_mut() {
        stdin.write_all(&chunk).await.map_err(|e| {
          capnp::Error::failed(format!("write to nix-store --import: {e}"))
        })?;
      }
      Ok(())
    })
  }

  fn close(
    self: capnp::capability::Rc<Self>,
    _params: output_sink::CloseParams,
    _results: output_sink::CloseResults,
  ) -> Promise<(), capnp::Error> {
    let inner = Arc::clone(&self.inner);
    Promise::from_future(async move {
      let (child, stdin) = {
        let mut state = inner.state.lock().await;
        state.closed = true;
        (state.child.take(), state.stdin.take())
      };
      // No chunk ever arrived
      let Some(mut child) = child else {
        return Ok(());
      };

      // Drop stdin to signal EOF so the import drains and exits.
      drop(stdin);
      let status = child.wait().await.map_err(|e| {
        capnp::Error::failed(format!("wait for nix-store --import: {e}"))
      })?;
      if !status.success() {
        return Err(capnp::Error::failed(format!(
          "nix-store --import failed for build {} ({status})",
          inner.build_id
        )));
      }
      Ok(())
    })
  }
}
