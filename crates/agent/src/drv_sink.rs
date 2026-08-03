//! `DrvSink` pipes an assigned derivation's closure into `nix-store --import`
//! on the agent. The mirror of the runner's `OutputSink`, letting the agent
//! obtain what it was told to build over the authenticated RPC instead of a
//! reachable binary cache.

use std::{process::Stdio, sync::Arc};

use capnp::capability::Promise;
use circus_proto::{drv_sink, limits};
use tokio::{
  io::AsyncWriteExt as _,
  process::{Child, ChildStdin},
  sync::Mutex,
};

use crate::sandbox::NixTool;

pub struct DrvSinkImpl {
  inner: Arc<Inner>,
}

struct Inner {
  drv_path: String,
  rootless: bool,
  state:    Mutex<State>,
}

struct State {
  child:  Option<Child>,
  stdin:  Option<ChildStdin>,
  closed: bool,
  total:  u64,
}

impl DrvSinkImpl {
  #[must_use]
  pub fn new(drv_path: String, rootless: bool) -> Self {
    Self {
      inner: Arc::new(Inner {
        drv_path,
        rootless,
        state: Mutex::new(State {
          child:  None,
          stdin:  None,
          closed: false,
          total:  0,
        }),
      }),
    }
  }
}

fn spawn_import(rootless: bool) -> color_eyre::Result<(Child, ChildStdin)> {
  let mut cmd = crate::sandbox::nix_command(rootless, NixTool::NixStore)?;
  cmd.arg("--import");
  let mut cmd = crate::sandbox::wrap_command(rootless, cmd)?;
  let mut child = cmd
    .stdin(Stdio::piped())
    .stdout(Stdio::null())
    .kill_on_drop(true)
    .spawn()?;
  let stdin = child.stdin.take().ok_or_else(|| {
    color_eyre::eyre::eyre!("nix-store --import produced no stdin")
  })?;
  Ok((child, stdin))
}

#[allow(refining_impl_trait_internal, refining_impl_trait_reachable)]
impl drv_sink::Server for DrvSinkImpl {
  #[expect(
    clippy::significant_drop_tightening,
    reason = "import child lock held across the chunk write"
  )]
  fn write(
    self: capnp::capability::Rc<Self>,
    params: drv_sink::WriteParams,
    _results: drv_sink::WriteResults,
  ) -> Promise<(), capnp::Error> {
    let inner = Arc::clone(&self.inner);
    Promise::from_future(async move {
      let pr = params.get()?;
      let chunk = pr.get_chunk()?.to_vec();
      if chunk.len() > limits::MAX_NAR_CHUNK_BYTES {
        return Err(capnp::Error::failed(format!(
          "drv chunk too large: {} > {}",
          chunk.len(),
          limits::MAX_NAR_CHUNK_BYTES
        )));
      }

      let mut state = inner.state.lock().await;
      if state.closed {
        return Err(capnp::Error::failed("drv sink is closed".into()));
      }
      state.total = state.total.saturating_add(chunk.len() as u64);
      if state.total > limits::MAX_IMPORT_TOTAL_BYTES {
        return Err(capnp::Error::failed(format!(
          "drv closure exceeds {} bytes for {}",
          limits::MAX_IMPORT_TOTAL_BYTES,
          inner.drv_path
        )));
      }
      if state.child.is_none() {
        let (child, stdin) = spawn_import(inner.rootless).map_err(|e| {
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
    _params: drv_sink::CloseParams,
    _results: drv_sink::CloseResults,
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
          "nix-store --import failed for {} ({status})",
          inner.drv_path
        )));
      }
      Ok(())
    })
  }
}
