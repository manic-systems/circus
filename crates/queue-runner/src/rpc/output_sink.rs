//! `OutputSink` pipes a build's output closure into `nix-store --import` on the
//! runner. `close` resolves once the import child exits, so the agent can await
//! it before reporting.

use std::{collections::HashSet, process::Stdio, sync::Arc, time::Duration};

use capnp::capability::Promise;
use circus_proto::{limits, output_sink};
use tokio::{
  io::AsyncWriteExt as _,
  process::{Child, ChildStdin, Command},
  sync::{Mutex, Semaphore},
};

use crate::{
  dispatch::{invalid_store_paths, try_read_drv_outputs},
  rpc::server::read_bounded_text_list,
};

pub struct OutputSinkImpl {
  inner: Arc<Inner>,
}

struct Inner {
  build_id: String,
  drv_path: String,
  is_fod:   bool,
  state:    Mutex<State>,
}

struct State {
  child:  Option<Child>,
  stdin:  Option<ChildStdin>,
  closed: bool,
  total:  u64,
}

impl OutputSinkImpl {
  #[must_use]
  pub fn new(build_id: String, drv_path: String, is_fod: bool) -> Self {
    Self {
      inner: Arc::new(Inner {
        build_id,
        drv_path,
        is_fod,
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

/// Caps concurrent substitutions so a burst of finishing builds does not fan
/// out into one download per build at once.
static SUBSTITUTE_PERMITS: Semaphore = Semaphore::const_new(4);
const SUBSTITUTE_TIMEOUT: Duration = Duration::from_mins(5);

/// Fetch what the runner's substituters have so the agent ships only the rest.
async fn substitute(inner: &Inner, missing: Vec<String>) -> Vec<String> {
  // A non-FOD build's own outputs exist only on the agent.
  let own_outputs = if inner.is_fod {
    HashSet::<String>::new()
  } else {
    match try_read_drv_outputs(&inner.drv_path).await {
      Ok(outputs) => outputs.into_iter().collect::<HashSet<String>>(),
      Err(e) => {
        tracing::warn!(build_id = %inner.build_id, error = %e, "query drv outputs");
        HashSet::<String>::new()
      },
    }
  };
  let candidates = missing
    .iter()
    .filter(|path| !own_outputs.contains(*path))
    .collect::<Vec<&String>>();
  if candidates.is_empty() {
    return missing;
  }

  let Ok(_permit) = SUBSTITUTE_PERMITS.acquire().await else {
    return missing;
  };
  let realise = realise(&inner.build_id, &candidates);
  if tokio::time::timeout(SUBSTITUTE_TIMEOUT, realise)
    .await
    .is_err()
  {
    tracing::warn!(
      build_id = %inner.build_id,
      "substitution timed out, asking the agent for the rest"
    );
  }

  match invalid_store_paths(&missing).await {
    Ok(still_missing) => still_missing,
    Err(e) => {
      tracing::warn!(
        build_id = %inner.build_id,
        error = %e,
        "validity recheck failed, asking for every missing path"
      );
      missing
    },
  }
}

async fn realise(build_id: &str, paths: &[&String]) {
  for batch in paths.chunks(1024) {
    let out = Command::new("nix-store")
      .args(["--realise", "--keep-going"])
      .args(batch)
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .kill_on_drop(true)
      .output()
      .await;
    match out {
      Ok(out) if !out.status.success() => {
        // Expected for anything the agent built that no cache has.
        tracing::debug!(
          build_id,
          status = %out.status,
          stderr = %String::from_utf8_lossy(&out.stderr).trim(),
          "some output closure paths are not substitutable"
        );
      },
      Ok(_) => {},
      Err(e) => {
        tracing::warn!(build_id, error = %e, "spawn nix-store --realise");
        return;
      },
    }
  }
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
      state.total = state.total.saturating_add(chunk.len() as u64);
      if state.total > limits::MAX_IMPORT_TOTAL_BYTES {
        return Err(capnp::Error::failed(format!(
          "imported closure exceeds {} bytes for build {}",
          limits::MAX_IMPORT_TOTAL_BYTES,
          inner.build_id
        )));
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

  fn missing(
    self: capnp::capability::Rc<Self>,
    params: output_sink::MissingParams,
    mut results: output_sink::MissingResults,
  ) -> Promise<(), capnp::Error> {
    let inner = Arc::clone(&self.inner);
    Promise::from_future(async move {
      let paths = read_bounded_text_list(
        params.get()?.get_paths()?,
        "paths",
        limits::MAX_CLOSURE_PATHS,
        limits::MAX_STORE_PATH_LEN,
      )?;
      let queried = paths.len();
      // Over-sending is harmless since import skips valid paths.
      let missing = match invalid_store_paths(&paths).await {
        Ok(missing) if missing.is_empty() => missing,
        Ok(missing) => substitute(&inner, missing).await,
        Err(e) => {
          tracing::warn!(
            build_id = %inner.build_id,
            error = %e,
            "validity check failed, asking for every queried path"
          );
          paths
        },
      };
      tracing::debug!(
        build_id = %inner.build_id,
        queried,
        missing = missing.len(),
        "output closure paths the runner lacks"
      );

      let mut out = results.get().init_missing(missing.len() as u32);
      for (idx, path) in missing.iter().enumerate() {
        out.set(idx as u32, path);
      }
      Ok(())
    })
  }
}
