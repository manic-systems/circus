//! One-shot build executor. Spawns `nix-store --realise`, streams stdout
//! and stderr through a `LogSink`, and assembles a `BuildResult` at exit.
use std::{
  collections::{BTreeMap, VecDeque},
  io,
  os::unix::process::ExitStatusExt,
  process::{ExitStatus, Stdio},
  time::{Duration, Instant},
};

use circus_proto::{log_sink, nix_log};
use tokio::{
  io::{AsyncBufReadExt, BufReader},
  process::Command,
  time::timeout,
};
use tokio_util::sync::CancellationToken;

use crate::sandbox::NixTool;

/// Keep log writes below the 1MiB wire cap without sending one RPC per line.
const MAX_LOG_BATCH_BYTES: usize = 256 * 1024;
const MAX_LOG_BATCH_LINES: usize = 4096;

/// Backpressure the child pipe instead of buffering logs without bound.
const LOG_CHANNEL_CAPACITY: usize = 1024;

#[derive(Clone, Copy)]
pub struct Tunables {
  pub drain_grace:   Duration,
  pub write_timeout: Duration,
  pub reap_timeout:  Duration,
}

impl Default for Tunables {
  fn default() -> Self {
    Self {
      drain_grace:   Duration::from_mins(5),
      write_timeout: Duration::from_mins(1),
      reap_timeout:  Duration::from_secs(30),
    }
  }
}

/// Per-build options handed down from the runner via the capnp schema.
pub struct BuildOptions<'a> {
  pub drv_path:          &'a str,
  pub max_log_size:      u64,
  pub max_silent_time:   Duration,
  pub build_timeout:     Duration,
  pub cores:             u32,
  pub extra_args:        Vec<String>,
  pub cache_substituter: String,
  pub cache_public_key:  String,
  pub rootless:          bool,
}

/// Everything needed to pull the assigned derivation from the runner. Kept
/// out of `BuildOptions` because that mirrors the schema's `BuildAssignment`.
pub struct DrvFetch<'a> {
  pub runner_cap: &'a circus_proto::runner::Client,
  pub machine_id: &'a str,
  pub build_id:   &'a str,
}

/// One output discovered after a successful realisation.
#[derive(Debug, Clone)]
pub struct ResolvedOutput {
  pub name: String,
  pub path: String,
}

/// Outcome accumulated from running the child process. Lifts into the
/// schema's `BuildResult` at the call site.
pub struct LocalResult {
  pub outcome:        circus_proto::BuildOutcome,
  pub exit_code:      i32,
  pub build_time_ms:  u64,
  pub upload_time_ms: u64,
  pub outputs:        Vec<ResolvedOutput>,
  pub error_message:  String,
}

/// Spawn the child, stream its log through `log_sink`, and wait for it.
///
/// `log_sink` is a Cap'n Proto client capability the runner created and
/// passed in via `Builder.assign`. We coalesce buffered log lines into
/// `write(chunk)` batches and call `close()` at the end.
///
/// `cancel` is signalled by [`crate::session::BuilderImpl::abort`]. When
/// it fires, the child is killed and the function returns an aborted
/// outcome.
///
/// # Errors
///
/// Returns the failure as a `LocalResult` with a non-success outcome rather
/// than `Result::Err`. Only IO failures around spawning the child are
/// raised as generic diagnostic errors.
pub async fn run(
  opts: BuildOptions<'_>,
  drv_fetch: DrvFetch<'_>,
  log_sink: log_sink::Client,
  cancel: CancellationToken,
) -> color_eyre::Result<LocalResult> {
  #![expect(
    clippy::future_not_send,
    reason = "capnp futures are not Send; agent uses a single-threaded runtime"
  )]
  // nix-store --realise requires the .drv in the local store and will not
  // fetch it from a substituter, so ask the runner that assigned it.
  if !drv_is_valid(&opts).await
    && let Err(e) = fetch_drv_over_rpc(&opts, &drv_fetch).await
  {
    if opts.cache_substituter.is_empty() {
      tracing::warn!(drv = %opts.drv_path, "drv fetch from runner failed: {e}");
    } else {
      tracing::warn!(
        drv = %opts.drv_path,
        "drv fetch from runner failed, falling back to cache: {e}"
      );
      fetch_drv_from_cache(&opts).await;
    }
  }
  let cmd = crate::sandbox::wrap_command(opts.rootless, build_command(&opts)?)?;
  run_command(cmd, &opts, Tunables::default(), log_sink, cancel).await
}

/// A valid path's references are themselves valid, so a present `.drv`
/// implies its whole input closure is too and nothing needs fetching.
async fn drv_is_valid(opts: &BuildOptions<'_>) -> bool {
  let Ok(mut cmd) =
    crate::sandbox::nix_command(opts.rootless, NixTool::NixStore)
  else {
    return false;
  };
  cmd.args(["--query", "--hash", opts.drv_path]);
  let Ok(mut cmd) = crate::sandbox::wrap_command(opts.rootless, cmd) else {
    return false;
  };
  matches!(
    cmd.stdin(Stdio::null()).output().await,
    Ok(o) if o.status.success()
  )
}

#[expect(
  clippy::future_not_send,
  reason = "capnp futures are not Send; agent uses a single-threaded runtime"
)]
async fn fetch_drv_over_rpc(
  opts: &BuildOptions<'_>,
  drv_fetch: &DrvFetch<'_>,
) -> color_eyre::Result<()> {
  let sink: circus_proto::drv_sink::Client = capnp_rpc::new_client(
    crate::drv_sink::DrvSinkImpl::new(opts.drv_path.to_owned(), opts.rootless),
  );
  let mut req = drv_fetch.runner_cap.fetch_drv_closure_request();
  {
    let mut p = req.get();
    p.set_machine_id(drv_fetch.machine_id);
    p.set_build_id(drv_fetch.build_id);
    p.set_sink(sink);
  }
  req.send().promise.await?;
  Ok(())
}

async fn fetch_drv_from_cache(opts: &BuildOptions<'_>) {
  let Ok(mut cmd) = crate::sandbox::nix_command(opts.rootless, NixTool::Nix)
  else {
    return;
  };
  cmd.args([
    "--extra-experimental-features",
    "nix-command",
    "copy",
    "--no-check-sigs",
    "--derivation",
    "--from",
    opts.cache_substituter.as_str(),
    opts.drv_path,
  ]);
  let Ok(mut cmd) = crate::sandbox::wrap_command(opts.rootless, cmd) else {
    return;
  };
  let output = cmd.stdin(Stdio::null()).output().await;
  match output {
    Ok(o) if o.status.success() => {},
    Ok(o) => {
      tracing::warn!(
        drv = %opts.drv_path,
        exit = o.status.code().unwrap_or(-1),
        stderr = %String::from_utf8_lossy(&o.stderr).trim(),
        "drv pre-fetch from cache failed"
      );
    },
    Err(e) => {
      tracing::warn!(drv = %opts.drv_path, "drv pre-fetch from cache failed to spawn: {e}");
    },
  }
}

fn build_command(opts: &BuildOptions<'_>) -> color_eyre::Result<Command> {
  let mut args = vec![
    "--realise".into(),
    "--log-format".into(),
    "internal-json".into(),
    opts.drv_path.into(),
  ];
  // Substitute the drv closure from the runner's cache.
  if !opts.cache_substituter.is_empty() {
    args.push("--option".into());
    args.push("extra-substituters".into());
    args.push(opts.cache_substituter.clone());
    if !opts.cache_public_key.is_empty() {
      args.push("--option".into());
      args.push("extra-trusted-public-keys".into());
      args.push(opts.cache_public_key.clone());
    }
  }
  if opts.cores > 0 {
    args.push("--option".into());
    args.push("cores".into());
    args.push(opts.cores.to_string());
  }
  args.extend(opts.extra_args.iter().cloned());

  let mut cmd = crate::sandbox::nix_command(opts.rootless, NixTool::NixStore)?;
  cmd
    .args(&args)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
  Ok(cmd)
}

enum Event {
  Line(Option<Result<String, String>>),
  ChildExited(io::Result<ExitStatus>),
  SilentTimeout,
  DeadlineTimeout,
  Cancelled,
}

/// Preserve a real child exit when it races log or deadline handling.
fn observe_exit(
  child: &mut tokio::process::Child,
  child_status: &mut Option<ExitStatus>,
  drain_deadline: &mut Option<Instant>,
  grace: Duration,
) -> bool {
  if child_status.is_none()
    && let Ok(Some(s)) = child.try_wait()
  {
    *drain_deadline = Some(Instant::now() + grace);
    *child_status = Some(s);
  }
  child_status.is_some()
}

#[expect(
  clippy::too_many_lines,
  clippy::future_not_send,
  reason = "one supervision loop with !Send capnp futures on a \
            single-threaded runtime"
)]
async fn run_command(
  mut cmd: Command,
  opts: &BuildOptions<'_>,
  tun: Tunables,
  log_sink: log_sink::Client,
  cancel: CancellationToken,
) -> color_eyre::Result<LocalResult> {
  let started = Instant::now();
  let mut child = cmd.spawn()?;
  let stdout = child
    .stdout
    .take()
    .ok_or_else(|| color_eyre::eyre::eyre!("child stdout missing"))?;
  let stderr = child
    .stderr
    .take()
    .ok_or_else(|| color_eyre::eyre::eyre!("child stderr missing"))?;

  // Drive stdout and stderr in two independent tasks so that EOF on one
  // stream does not cause lines buffered in the other to be discarded.
  // Both tasks forward lines (or IO errors) through a shared channel.
  let (line_tx, mut line_rx) =
    tokio::sync::mpsc::channel::<Result<String, String>>(LOG_CHANNEL_CAPACITY);
  {
    let tx = line_tx.clone();
    let mut reader = BufReader::new(stdout).lines();
    tokio::spawn(async move {
      loop {
        match reader.next_line().await {
          Ok(Some(line)) => {
            if tx.send(Ok(line)).await.is_err() {
              break;
            }
          },
          Ok(None) => break,
          Err(e) => {
            let _ = tx.send(Err(format!("stdout read: {e}"))).await;
            break;
          },
        }
      }
    });
  }
  {
    let tx = line_tx.clone();
    let mut reader = BufReader::new(stderr).lines();
    tokio::spawn(async move {
      loop {
        match reader.next_line().await {
          Ok(Some(line)) => {
            if tx.send(Ok(line)).await.is_err() {
              break;
            }
          },
          Ok(None) => break,
          Err(e) => {
            let _ = tx.send(Err(format!("stderr read: {e}"))).await;
            break;
          },
        }
      }
    });
  }
  // Drop the original sender so the channel closes once both reader tasks end.
  drop(line_tx);

  let mut bytes_sent: u64 = 0;
  let mut error_message = String::new();
  let mut log_size_exceeded = false;
  let mut sink_failed = false;
  let mut log_truncated = false;
  let mut aborted = false;
  let mut timed_out = false;
  let mut killed = false;
  let mut child_status = Option::<ExitStatus>::None;
  let mut drain_deadline = Option::<Instant>::None;

  let overall_deadline = if opts.build_timeout.is_zero() {
    None
  } else {
    Some(Instant::now() + opts.build_timeout)
  };
  let mut last_output = Instant::now();
  let mut recent_msgs = VecDeque::<String>::with_capacity(32);

  loop {
    let pre_exit = child_status.is_none();
    let read_timeout = if pre_exit {
      remaining_silent(&opts.max_silent_time, last_output)
    } else {
      None
    };
    let phase_deadline = if pre_exit {
      overall_deadline
    } else {
      drain_deadline
    };
    let deadline_timeout = phase_deadline
      .map(|deadline| deadline.saturating_duration_since(Instant::now()))
      .map(|remaining| remaining.max(Duration::from_millis(1)));

    let ev = tokio::select! {
      r = line_rx.recv() => Event::Line(r),
      s = child.wait(), if pre_exit => Event::ChildExited(s),
      () = sleep_opt(read_timeout), if pre_exit => Event::SilentTimeout,
      () = sleep_opt(deadline_timeout) => Event::DeadlineTimeout,
      () = cancel.cancelled() => Event::Cancelled,
    };

    let first = match ev {
      Event::ChildExited(s) => {
        let status = s?;
        if child_status.is_none() {
          drain_deadline = Some(Instant::now() + tun.drain_grace);
          child_status = Some(status);
        }
        continue;
      },
      Event::SilentTimeout => {
        if observe_exit(
          &mut child,
          &mut child_status,
          &mut drain_deadline,
          tun.drain_grace,
        ) {
          continue;
        }
        error_message = "max-silent-time exceeded".into();
        let _ = child.start_kill();
        killed = true;
        break;
      },
      Event::DeadlineTimeout => {
        if pre_exit
          && observe_exit(
            &mut child,
            &mut child_status,
            &mut drain_deadline,
            tun.drain_grace,
          )
        {
          continue;
        }
        if child_status.is_none() {
          timed_out = true;
          error_message = "build-timeout exceeded".into();
          let _ = child.start_kill();
          killed = true;
        } else {
          log_truncated = true;
        }
        break;
      },
      Event::Cancelled => {
        if observe_exit(
          &mut child,
          &mut child_status,
          &mut drain_deadline,
          tun.drain_grace,
        ) {
          log_truncated = true;
          break;
        }
        aborted = true;
        error_message = "aborted by runner".into();
        let _ = child.start_kill();
        killed = true;
        break;
      },
      Event::Line(None) => break,
      Event::Line(Some(Err(e))) => {
        error_message = e;
        break;
      },
      Event::Line(Some(Ok(l))) => l,
    };
    last_output = Instant::now();

    let mut batch = Vec::<u8>::new();
    let mut batch_lines = 0usize;
    let mut pending_read_err = Option::<String>::None;
    let mut next = Some(first);
    while let Some(line) = next.take() {
      if let Some(nix_log::LogLine::Message { text, .. }) =
        nix_log::parse_line(&line)
      {
        if recent_msgs.len() == 32 {
          recent_msgs.pop_front();
        }
        recent_msgs.push_back(text);
      }

      if !log_size_exceeded && !log_truncated && !sink_failed {
        if bytes_sent.saturating_add(line.len() as u64 + 1) > opts.max_log_size
        {
          if child_status.is_none() {
            log_size_exceeded = true;
            error_message = "max-log-size exceeded".into();
            let _ = child.start_kill();
            killed = true;
          } else {
            log_truncated = true;
          }
        } else {
          bytes_sent = bytes_sent.saturating_add(line.len() as u64 + 1);
          if !batch.is_empty() {
            batch.push(b'\n');
          }
          batch.extend_from_slice(line.as_bytes());
          batch_lines += 1;
        }
      }

      if batch.len() >= MAX_LOG_BATCH_BYTES
        || batch_lines >= MAX_LOG_BATCH_LINES
      {
        break;
      }
      match line_rx.try_recv() {
        Ok(Ok(l)) => next = Some(l),
        Ok(Err(e)) => {
          pending_read_err = Some(e);
          break;
        },
        Err(_) => break,
      }
    }

    if !batch.is_empty() {
      // Bound each write so a wedged sink cannot stall every deadline
      let write_cap = phase_deadline
        .map_or(tun.write_timeout, |deadline| {
          deadline
            .saturating_duration_since(Instant::now())
            .min(tun.write_timeout)
        })
        .max(Duration::from_millis(1));
      let write_result = tokio::select! {
        r = timeout(write_cap, forward_chunk(&log_sink, &batch)) => match r {
          Ok(Ok(())) => Ok(()),
          Ok(Err(e)) => Err(format!("log sink write failed: {e}")),
          Err(_) => Err("log sink write timed out".into()),
        },
        () = cancel.cancelled(), if child_status.is_none() => {
          if observe_exit(
            &mut child,
            &mut child_status,
            &mut drain_deadline,
            tun.drain_grace,
          ) {
            log_truncated = true;
            break;
          }
          aborted = true;
          error_message = "aborted by runner".into();
          let _ = child.start_kill();
          killed = true;
          break;
        }
      };
      if let Err(e) = write_result {
        if observe_exit(
          &mut child,
          &mut child_status,
          &mut drain_deadline,
          tun.drain_grace,
        ) {
          log_truncated = true;
          break;
        }
        tracing::warn!(error = %e, "log sink write failed; killing child");
        sink_failed = true;
        let _ = child.start_kill();
        killed = true;
      }
    }

    if let Some(e) = pending_read_err {
      error_message = e;
      break;
    }
    let deadline = if child_status.is_none() {
      overall_deadline
    } else {
      drain_deadline
    };
    if let Some(deadline) = deadline
      && Instant::now() >= deadline
    {
      let was_pre_exit = child_status.is_none();
      if was_pre_exit
        && observe_exit(
          &mut child,
          &mut child_status,
          &mut drain_deadline,
          tun.drain_grace,
        )
        && drain_deadline.is_some_and(|d| Instant::now() < d)
      {
        continue;
      }
      if child_status.is_none() {
        timed_out = true;
        error_message = "build-timeout exceeded".into();
        let _ = child.start_kill();
        killed = true;
      } else {
        log_truncated = true;
      }
      break;
    }
  }

  let status = match child_status {
    Some(s) => Some(s),
    None if killed => {
      if let Ok(s) = timeout(tun.reap_timeout, child.wait()).await {
        Some(s?)
      } else {
        let _ = child.kill().await;
        None
      }
    },
    // EOF with a live child still waits on the build deadline.
    None => {
      match overall_deadline {
        Some(deadline) => {
          if let Ok(s) = timeout(
            deadline.saturating_duration_since(Instant::now()),
            child.wait(),
          )
          .await
          {
            Some(s?)
          } else {
            timed_out = true;
            error_message = "build-timeout exceeded".into();
            let _ = child.kill().await;
            None
          }
        },
        None => Some(child.wait().await?),
      }
    },
  };

  if log_truncated {
    let _ = timeout(
      tun.write_timeout,
      forward_chunk(&log_sink, b"[circus-agent] log truncated"),
    )
    .await;
    if !error_message.is_empty() {
      error_message.push('\n');
    }
    error_message
      .push_str("log drain timed out after child exit; log truncated");
  }
  let _ = timeout(tun.write_timeout, close_log(&log_sink)).await;

  let Some(status) = status else {
    return Ok(LocalResult {
      outcome: if timed_out {
        circus_proto::BuildOutcome::TimedOut
      } else if aborted {
        circus_proto::BuildOutcome::Aborted
      } else {
        circus_proto::BuildOutcome::BuildFailure
      },
      exit_code: -1,
      build_time_ms: started.elapsed().as_millis() as u64,
      upload_time_ms: 0,
      outputs: Vec::new(),
      error_message,
    });
  };

  let exit_code = status.code().unwrap_or(-1);
  let oom_killed =
    !killed && !aborted && !timed_out && matches!(status.signal(), Some(9));
  let success = status.success()
    && !log_size_exceeded
    && !sink_failed
    && !aborted
    && !timed_out
    && !oom_killed;
  if !success && error_message.is_empty() {
    error_message = summarize_failure(&recent_msgs);
  }
  let outcome = if success {
    circus_proto::BuildOutcome::Success
  } else if timed_out {
    circus_proto::BuildOutcome::TimedOut
  } else if aborted {
    circus_proto::BuildOutcome::Aborted
  } else if oom_killed {
    circus_proto::BuildOutcome::OomKilled
  } else {
    circus_proto::BuildOutcome::BuildFailure
  };

  let outputs = if success {
    query_outputs(opts.drv_path, opts.rootless).await
  } else {
    Vec::new()
  };

  Ok(LocalResult {
    outcome,
    exit_code,
    build_time_ms: started.elapsed().as_millis() as u64,
    upload_time_ms: 0,
    outputs,
    error_message,
  })
}

fn remaining_silent(
  max_silent: &Duration,
  last_output: Instant,
) -> Option<Duration> {
  if max_silent.is_zero() {
    return None;
  }
  Some(
    max_silent
      .saturating_sub(last_output.elapsed())
      .max(Duration::from_millis(1)),
  )
}

async fn sleep_opt(d: Option<Duration>) {
  match d {
    Some(d) => tokio::time::sleep(d).await,
    None => std::future::pending().await,
  }
}

/// Query all outputs of the derivation after a successful realisation.
///
/// `nix derivation show --derivation` returns the structured outputs map
/// with output names as keys. We fall back to `nix-store --query --outputs`
/// (which only gives paths, not names) if that fails.
async fn query_outputs(drv_path: &str, rootless: bool) -> Vec<ResolvedOutput> {
  if let Some(parsed) = query_outputs_via_show(drv_path, rootless).await {
    return parsed;
  }
  let Ok(mut cmd) = crate::sandbox::nix_command(rootless, NixTool::NixStore)
    .and_then(|cmd| {
      crate::sandbox::wrap_command(rootless, cmd).map_err(Into::into)
    })
  else {
    return Vec::new();
  };
  match cmd.args(["--query", "--outputs", drv_path]).output().await {
    Ok(out) if out.status.success() => {
      String::from_utf8_lossy(&out.stdout)
        .lines()
        .enumerate()
        .filter_map(|(i, l)| {
          let p = l.trim();
          if p.is_empty() {
            None
          } else {
            Some(ResolvedOutput {
              name: if i == 0 {
                "out".into()
              } else {
                format!("out{i}")
              },
              path: p.to_owned(),
            })
          }
        })
        .collect()
    },
    _ => Vec::new(),
  }
}

/// Join the last few nix messages into a capped error summary.
fn summarize_failure(msgs: &VecDeque<String>) -> String {
  const MAX: usize = 4096;
  let mut tail = msgs
    .iter()
    .rev()
    .take(10)
    .map(String::as_str)
    .collect::<Vec<&str>>();
  tail.reverse();
  let s = tail.join("\n");
  match s.char_indices().nth_back(MAX - 1) {
    Some((cut, _)) => format!("…{}", &s[cut..]),
    None => s,
  }
}

async fn query_outputs_via_show(
  drv_path: &str,
  rootless: bool,
) -> Option<Vec<ResolvedOutput>> {
  let cmd = crate::sandbox::nix_command(rootless, NixTool::Nix).ok()?;
  let mut cmd = crate::sandbox::wrap_command(rootless, cmd).ok()?;
  let out = cmd
    .args([
      "--extra-experimental-features",
      "nix-command",
      "derivation",
      "show",
      drv_path,
    ])
    .output()
    .await
    .ok()?;
  if !out.status.success() {
    return None;
  }
  let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
  let top = v.as_object()?;
  let drv = top.values().next()?.as_object()?;
  let outputs = drv.get("outputs")?.as_object()?;
  let mut keyed: BTreeMap<String, String> = BTreeMap::new();
  for (name, info) in outputs {
    let path = info.as_object()?.get("path")?.as_str()?.to_owned();
    keyed.insert(name.clone(), path);
  }
  Some(
    keyed
      .into_iter()
      .map(|(name, path)| ResolvedOutput { name, path })
      .collect(),
  )
}

async fn forward_chunk(
  sink: &log_sink::Client,
  chunk: &[u8],
) -> Result<(), capnp::Error> {
  #![expect(
    clippy::future_not_send,
    reason = "capnp futures are not Send; agent uses a single-threaded runtime"
  )]
  let mut req = sink.write_request();
  req.get().set_chunk(chunk);
  req.send().promise.await?;
  Ok(())
}

async fn close_log(sink: &log_sink::Client) -> Result<(), capnp::Error> {
  #![expect(
    clippy::future_not_send,
    reason = "capnp futures are not Send; agent uses a single-threaded runtime"
  )]
  sink.close_request().send().promise.await?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use std::{cell::RefCell, future::Future, rc::Rc};

  use capnp::capability::Promise;
  use circus_proto::log_sink;

  use super::*;

  struct TestSink {
    chunks:     Rc<RefCell<Vec<Vec<u8>>>>,
    hang_after: Option<usize>,
  }

  #[allow(refining_impl_trait_internal)]
  impl log_sink::Server for TestSink {
    fn write(
      self: capnp::capability::Rc<Self>,
      params: log_sink::WriteParams,
      _results: log_sink::WriteResults,
    ) -> Promise<(), capnp::Error> {
      let chunks = Rc::clone(&self.chunks);
      let hang_after = self.hang_after;
      Promise::from_future(async move {
        let n = {
          let mut g = chunks.borrow_mut();
          g.push(params.get()?.get_chunk()?.to_vec());
          g.len()
        };
        if hang_after.is_some_and(|h| n > h) {
          std::future::pending::<()>().await;
        }
        Ok(())
      })
    }

    fn close(
      self: capnp::capability::Rc<Self>,
      _params: log_sink::CloseParams,
      _results: log_sink::CloseResults,
    ) -> Promise<(), capnp::Error> {
      Promise::ok(())
    }
  }

  struct Harness {
    chunks: Rc<RefCell<Vec<Vec<u8>>>>,
    sink:   log_sink::Client,
  }

  fn harness(hang_after: Option<usize>) -> Harness {
    let chunks = Rc::new(RefCell::new(Vec::new()));
    let sink = capnp_rpc::new_client(TestSink {
      chunks: Rc::clone(&chunks),
      hang_after,
    });
    Harness { chunks, sink }
  }

  fn sh(script: &str) -> Command {
    let mut cmd = Command::new("sh");
    cmd
      .args(["-c", script])
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .kill_on_drop(true);
    cmd
  }

  fn opts(max_log_size: u64, build_timeout: Duration) -> BuildOptions<'static> {
    BuildOptions {
      drv_path: "/nix/store/00000000000000000000000000000000-test.drv",
      max_log_size,
      max_silent_time: Duration::ZERO,
      build_timeout,
      cores: 0,
      extra_args: Vec::new(),
      cache_substituter: String::new(),
      cache_public_key: String::new(),
      rootless: false,
    }
  }

  fn fast_tunables() -> Tunables {
    Tunables {
      drain_grace:   Duration::from_secs(10),
      write_timeout: Duration::from_millis(200),
      reap_timeout:  Duration::from_secs(5),
    }
  }

  fn run_local<F: Future>(f: F) -> F::Output {
    let rt = tokio::runtime::Builder::new_current_thread()
      .enable_all()
      .build()
      .expect("runtime");
    let local = tokio::task::LocalSet::new();
    rt.block_on(local.run_until(f))
  }

  #[test]
  fn chatty_log_is_batched_and_intact() {
    run_local(async {
      let h = harness(None);
      let result = run_command(
        sh("seq 20000"),
        &opts(u64::MAX, Duration::from_mins(1)),
        fast_tunables(),
        h.sink,
        CancellationToken::new(),
      )
      .await
      .expect("run_command");

      assert_eq!(result.outcome, circus_proto::BuildOutcome::Success);
      assert_eq!(result.exit_code, 0);
      let chunks = h.chunks.borrow();
      // The exact batch count depends on Tokio's cooperative scheduling.
      assert!(
        chunks.len() < 1000,
        "expected batching, got {}",
        chunks.len()
      );
      let joined = chunks
        .iter()
        .map(|c| String::from_utf8(c.clone()).expect("utf8"))
        .collect::<Vec<_>>()
        .join("\n");
      let want = (1..=20000).map(|i| i.to_string()).collect::<Vec<_>>();
      assert_eq!(joined.lines().collect::<Vec<_>>(), want);
    });
  }

  #[test]
  fn finished_build_survives_wedged_sink() {
    run_local(async {
      // This exits before the first write wedges.
      let h = harness(Some(0));
      let result = run_command(
        sh("seq 100"),
        &opts(u64::MAX, Duration::from_mins(1)),
        fast_tunables(),
        h.sink,
        CancellationToken::new(),
      )
      .await
      .expect("run_command");

      assert_eq!(result.outcome, circus_proto::BuildOutcome::Success);
      assert_eq!(result.exit_code, 0);
      assert!(
        result.error_message.contains("log truncated"),
        "missing truncation note: {:?}",
        result.error_message
      );
    });
  }

  #[test]
  fn build_timeout_kills_hung_child() {
    run_local(async {
      let h = harness(None);
      let result = run_command(
        sh("sleep 30"),
        &opts(u64::MAX, Duration::from_millis(300)),
        fast_tunables(),
        h.sink,
        CancellationToken::new(),
      )
      .await
      .expect("run_command");

      assert_eq!(result.outcome, circus_proto::BuildOutcome::TimedOut);
      assert_eq!(result.exit_code, -1);
      assert!(result.error_message.contains("build-timeout"));
    });
  }

  #[test]
  fn max_log_size_fails_running_build() {
    run_local(async {
      let h = harness(None);
      let result = run_command(
        sh("seq 10000; sleep 30"),
        &opts(64, Duration::from_mins(1)),
        fast_tunables(),
        h.sink,
        CancellationToken::new(),
      )
      .await
      .expect("run_command");

      assert_eq!(result.outcome, circus_proto::BuildOutcome::BuildFailure);
      assert!(result.error_message.contains("max-log-size exceeded"));
    });
  }
}
