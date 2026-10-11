//! One-shot build executor. Builds through `nix-daemon --stdio`, streams its
//! log through a [`log_sink::Client`], and assembles a [`LocalResult`] from
//! Nix's verdict.
use std::{
  collections::VecDeque,
  path::PathBuf,
  process::Stdio,
  time::{Duration, Instant},
};

use circus_proto::{log_sink, nix_log};
use nix::sys::statvfs::statvfs;
use tokio::{
  io::{AsyncBufReadExt, AsyncRead, AsyncWrite, BufReader, BufWriter},
  process::{Child, Command},
  sync::mpsc,
  task::{JoinError, JoinHandle},
  time::{MissedTickBehavior, interval, timeout},
};
use tokio_util::sync::CancellationToken;

use crate::{
  daemon::{BuildMode, BuildResult, BuildStatus, Connection, DaemonError},
  sandbox::NixTool,
};

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
  // Nix requires the .drv in the local store and will not fetch it from a
  // substituter, so ask the runner that assigned it.
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
  let (cmd, mode) = daemon_command(&opts)?;
  let mut cmd = crate::sandbox::wrap_command(opts.rootless, cmd)?;
  cmd.stdin(Stdio::piped());
  run_command(cmd, mode, &opts, Tunables::default(), log_sink, cancel).await
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

/// Settings go on the daemon's command line instead of through `SetOptions`,
/// so a stdio daemon proxying to the system one forwards them under the same
/// trust rules `nix-store` had.
fn daemon_command(
  opts: &BuildOptions<'_>,
) -> color_eyre::Result<(Command, BuildMode)> {
  let mut args = vec![
    "--stdio".to_owned(),
    "--option".into(),
    "builders".into(),
    String::new(),
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

  let mut mode = BuildMode::Normal;
  for arg in &opts.extra_args {
    match arg.as_str() {
      "--rebuild" | "--check" => mode = BuildMode::Check,
      "--repair" => mode = BuildMode::Repair,
      "-L" | "--print-build-logs" => {},
      _ => args.push(arg.clone()),
    }
  }

  let mut cmd = crate::sandbox::nix_command(opts.rootless, NixTool::NixDaemon)?;
  cmd
    .args(&args)
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
  Ok((cmd, mode))
}

struct Realised {
  result:  BuildResult,
  outputs: Vec<ResolvedOutput>,
}

async fn realise<R, W>(
  reader: R,
  writer: W,
  drv_path: String,
  mode: BuildMode,
  log: mpsc::Sender<Result<String, String>>,
) -> Result<Realised, DaemonError>
where
  R: AsyncRead + Unpin,
  W: AsyncWrite + Unpin,
{
  let mut conn =
    Connection::handshake(BufReader::new(reader), BufWriter::new(writer), log)
      .await?;
  let result = conn.build(&drv_path, mode).await?;
  let outputs = if result.status.succeeded() {
    conn
      .output_paths(&drv_path)
      .await?
      .into_iter()
      .map(|(name, path)| ResolvedOutput { name, path })
      .collect()
  } else {
    Vec::new()
  };
  Ok(Realised { result, outputs })
}

fn error_chain(error: &dyn std::error::Error) -> String {
  let mut text = error.to_string();
  let mut source = error.source();
  while let Some(cause) = source {
    text.push_str(": ");
    text.push_str(&cause.to_string());
    source = cause.source();
  }
  text
}

enum Event {
  Line(Option<Result<String, String>>),
  Finished(Result<Result<Realised, DaemonError>, JoinError>),
  SilentTimeout,
  DeadlineTimeout,
  Cancelled,
  DiskSample,
}

async fn run_command(
  mut cmd: Command,
  mode: BuildMode,
  opts: &BuildOptions<'_>,
  tun: Tunables,
  log_sink: log_sink::Client,
  cancel: CancellationToken,
) -> color_eyre::Result<LocalResult> {
  #![expect(
    clippy::future_not_send,
    reason = "capnp futures are not Send; agent uses a single-threaded runtime"
  )]
  let mut child = cmd.spawn()?;
  let stdin = child
    .stdin
    .take()
    .ok_or_else(|| color_eyre::eyre::eyre!("nix-daemon stdin missing"))?;
  let stdout = child
    .stdout
    .take()
    .ok_or_else(|| color_eyre::eyre::eyre!("nix-daemon stdout missing"))?;
  supervise(child, stdout, stdin, mode, opts, tun, log_sink, cancel).await
}

/// Record a protocol result that finished while a deadline or abort raced it,
/// so a completed build is not reported as killed.
async fn observe_finish(
  protocol: &mut JoinHandle<Result<Realised, DaemonError>>,
  realised: &mut Option<Result<Realised, String>>,
  drain_deadline: &mut Option<Instant>,
  grace: Duration,
) -> bool {
  if realised.is_none() && protocol.is_finished() {
    *realised = Some(joined(protocol.await));
    *drain_deadline = Some(Instant::now() + grace);
  }
  realised.is_some()
}

fn joined(
  result: Result<Result<Realised, DaemonError>, JoinError>,
) -> Result<Realised, String> {
  match result {
    Ok(realised) => realised.map_err(|e| error_chain(&e)),
    Err(e) => Err(format!("nix-daemon session ended: {e}")),
  }
}

#[expect(
  clippy::too_many_lines,
  clippy::too_many_arguments,
  clippy::future_not_send,
  reason = "one supervision loop with !Send capnp futures on a \
            single-threaded runtime"
)]
async fn supervise<R, W>(
  mut child: Child,
  reader: R,
  writer: W,
  mode: BuildMode,
  opts: &BuildOptions<'_>,
  tun: Tunables,
  log_sink: log_sink::Client,
  cancel: CancellationToken,
) -> color_eyre::Result<LocalResult>
where
  R: AsyncRead + Unpin + Send + 'static,
  W: AsyncWrite + Unpin + Send + 'static,
{
  let started = Instant::now();
  let oom_kills_before = kernel_oom_kills();
  let stderr = child
    .stderr
    .take()
    .ok_or_else(|| color_eyre::eyre::eyre!("nix-daemon stderr missing"))?;

  // The protocol task and the daemon's own stderr share one channel, which
  // closes once both are done.
  let (line_tx, mut line_rx) =
    mpsc::channel::<Result<String, String>>(LOG_CHANNEL_CAPACITY);
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
  let mut protocol = tokio::spawn(realise(
    reader,
    writer,
    opts.drv_path.to_owned(),
    mode,
    line_tx,
  ));

  let mut bytes_sent: u64 = 0;
  let mut error_message = String::new();
  let mut log_size_exceeded = false;
  let mut silent_too_long = false;
  let mut builder_started = false;
  let mut sink_failed = false;
  let mut log_truncated = false;
  let mut aborted = false;
  let mut timed_out = false;
  let mut killed = false;
  let mut realised = Option::<Result<Realised, String>>::None;
  let mut drain_deadline = Option::<Instant>::None;

  let overall_deadline = if opts.build_timeout.is_zero() {
    None
  } else {
    Some(Instant::now() + opts.build_timeout)
  };
  let mut last_output = Instant::now();
  let mut recent_msgs = VecDeque::<String>::with_capacity(32);
  let mut disk_samples = interval(DISK_SAMPLE_INTERVAL);
  disk_samples.set_missed_tick_behavior(MissedTickBehavior::Skip);
  let mut lowest_free = Option::<(PathBuf, u64)>::None;

  loop {
    let pre_finish = realised.is_none();
    let read_timeout = if pre_finish {
      remaining_silent(&opts.max_silent_time, last_output)
    } else {
      None
    };
    let phase_deadline = if pre_finish {
      overall_deadline
    } else {
      drain_deadline
    };
    let deadline_timeout = phase_deadline
      .map(|deadline| deadline.saturating_duration_since(Instant::now()))
      .map(|remaining| remaining.max(Duration::from_millis(1)));

    let ev = tokio::select! {
      r = line_rx.recv() => Event::Line(r),
      r = &mut protocol, if pre_finish => Event::Finished(r),
      () = sleep_opt(read_timeout), if pre_finish => Event::SilentTimeout,
      () = sleep_opt(deadline_timeout) => Event::DeadlineTimeout,
      () = cancel.cancelled() => Event::Cancelled,
      _ = disk_samples.tick(), if pre_finish => Event::DiskSample,
    };

    let first = match ev {
      Event::DiskSample => {
        lowest_free = lower_free_space(lowest_free, lowest_free_space());
        continue;
      },
      Event::Finished(r) => {
        realised = Some(joined(r));
        drain_deadline = Some(Instant::now() + tun.drain_grace);
        drop(child.stdin.take());
        continue;
      },
      Event::SilentTimeout => {
        if observe_finish(
          &mut protocol,
          &mut realised,
          &mut drain_deadline,
          tun.drain_grace,
        )
        .await
        {
          continue;
        }
        silent_too_long = true;
        error_message = "max-silent-time exceeded".into();
        hang_up(&protocol, &mut child);
        killed = true;
        break;
      },
      Event::DeadlineTimeout => {
        if pre_finish
          && observe_finish(
            &mut protocol,
            &mut realised,
            &mut drain_deadline,
            tun.drain_grace,
          )
          .await
        {
          continue;
        }
        if realised.is_none() {
          timed_out = true;
          error_message = "build-timeout exceeded".into();
          hang_up(&protocol, &mut child);
          killed = true;
        } else {
          log_truncated = true;
        }
        break;
      },
      Event::Cancelled => {
        if observe_finish(
          &mut protocol,
          &mut realised,
          &mut drain_deadline,
          tun.drain_grace,
        )
        .await
        {
          log_truncated = true;
          break;
        }
        aborted = true;
        error_message = "aborted by runner".into();
        hang_up(&protocol, &mut child);
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
      match nix_log::parse_line(&line) {
        Some(nix_log::LogLine::Message { text, .. }) => {
          remember(&mut recent_msgs, text);
        },
        Some(nix_log::LogLine::BuildStarted) => builder_started = true,
        None if !nix_log::is_envelope(&line) => {
          remember(&mut recent_msgs, line.clone());
        },
        _ => {},
      }

      if !log_size_exceeded
        && !log_truncated
        && !sink_failed
        && !nix_log::is_progress(&line)
      {
        if bytes_sent.saturating_add(line.len() as u64 + 1) > opts.max_log_size
        {
          if realised.is_none() {
            log_size_exceeded = true;
            error_message = "max-log-size exceeded".into();
            hang_up(&protocol, &mut child);
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
        () = cancel.cancelled(), if realised.is_none() => {
          if observe_finish(
            &mut protocol,
            &mut realised,
            &mut drain_deadline,
            tun.drain_grace,
          )
          .await
          {
            log_truncated = true;
            break;
          }
          aborted = true;
          error_message = "aborted by runner".into();
          hang_up(&protocol, &mut child);
          killed = true;
          break;
        }
      };
      if let Err(e) = write_result {
        if observe_finish(
          &mut protocol,
          &mut realised,
          &mut drain_deadline,
          tun.drain_grace,
        )
        .await
        {
          log_truncated = true;
          break;
        }
        tracing::warn!(error = %e, "log sink write failed; hanging up on nix-daemon");
        sink_failed = true;
        hang_up(&protocol, &mut child);
        killed = true;
      }
    }

    if let Some(e) = pending_read_err {
      error_message = e;
      break;
    }
    let deadline = if realised.is_none() {
      overall_deadline
    } else {
      drain_deadline
    };
    if let Some(deadline) = deadline
      && Instant::now() >= deadline
    {
      let was_pre_finish = realised.is_none();
      if was_pre_finish
        && observe_finish(
          &mut protocol,
          &mut realised,
          &mut drain_deadline,
          tun.drain_grace,
        )
        .await
        && drain_deadline.is_some_and(|d| Instant::now() < d)
      {
        continue;
      }
      if realised.is_none() {
        timed_out = true;
        error_message = "build-timeout exceeded".into();
        hang_up(&protocol, &mut child);
        killed = true;
      } else {
        log_truncated = true;
      }
      break;
    }
  }

  // A closed log stream with the session still running waits on the build
  // deadline.
  if realised.is_none() && !killed {
    let remaining = overall_deadline
      .map(|deadline| deadline.saturating_duration_since(Instant::now()));
    match remaining {
      Some(remaining) => {
        if let Ok(r) = timeout(remaining, &mut protocol).await {
          realised = Some(joined(r));
        } else {
          timed_out = true;
          error_message = "build-timeout exceeded".into();
          hang_up(&protocol, &mut child);
          killed = true;
        }
      },
      None => realised = Some(joined((&mut protocol).await)),
    }
  }
  drop(child.stdin.take());
  let daemon_exit =
    if let Ok(status) = timeout(tun.reap_timeout, child.wait()).await {
      status.ok()
    } else {
      let _ = child.kill().await;
      None
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
      .push_str("log drain timed out after the build finished; log truncated");
  }
  let _ = timeout(tun.write_timeout, close_log(&log_sink)).await;

  let failed =
    |status: Option<BuildStatus>| status.map_or(1, BuildStatus::exit_status);
  let (outcome, exit_code, outputs) = if killed && timed_out {
    (circus_proto::BuildOutcome::TimedOut, -1, Vec::new())
  } else if killed && aborted {
    (circus_proto::BuildOutcome::Aborted, -1, Vec::new())
  } else if sink_failed {
    (circus_proto::BuildOutcome::InfraFailure, -1, Vec::new())
  } else if silent_too_long || log_size_exceeded {
    (circus_proto::BuildOutcome::BuildFailure, -1, Vec::new())
  } else {
    match realised {
      Some(Ok(Realised { result, outputs })) if result.status.succeeded() => {
        (circus_proto::BuildOutcome::Success, 0, outputs)
      },
      Some(Ok(Realised { result, .. })) => {
        if error_message.is_empty() {
          error_message = result.error;
        }
        let oom_killed = oom_kills_before
          .zip(kernel_oom_kills())
          .is_some_and(|(before, after)| after > before);
        let outcome = if oom_killed {
          circus_proto::BuildOutcome::OomKilled
        } else {
          failure_outcome(
            result.status,
            builder_started,
            lower_free_space(lowest_free, lowest_free_space()),
            &mut error_message,
          )
        };
        (outcome, failed(Some(result.status)), Vec::new())
      },
      Some(Err(e)) => {
        if !error_message.is_empty() {
          error_message.push('\n');
        }
        error_message.push_str(&e);
        if let Some(status) = daemon_exit.filter(|status| !status.success()) {
          let exited = format!(", and nix-daemon {status}");
          error_message.push_str(&exited);
        }
        let stderr = summarize_failure(&recent_msgs);
        if !stderr.is_empty() {
          error_message.push('\n');
          error_message.push_str(&stderr);
        }
        (
          circus_proto::BuildOutcome::InfraFailure,
          failed(None),
          Vec::new(),
        )
      },
      None => (circus_proto::BuildOutcome::InfraFailure, -1, Vec::new()),
    }
  };
  if outcome != circus_proto::BuildOutcome::Success && error_message.is_empty()
  {
    error_message = summarize_failure(&recent_msgs);
  }

  Ok(LocalResult {
    outcome,
    exit_code,
    build_time_ms: started.elapsed().as_millis() as u64,
    upload_time_ms: 0,
    outputs,
    error_message,
  })
}

/// The daemon kills the builder when its client hangs up. A test child that
/// stands in for the daemon exits once its stdin closes.
fn hang_up(
  protocol: &JoinHandle<Result<Realised, DaemonError>>,
  child: &mut Child,
) {
  protocol.abort();
  drop(child.stdin.take());
}

/// The runner dispatches only once dependencies succeed, so a failed
/// dependency is one the agent could not substitute.
fn failure_outcome(
  status: BuildStatus,
  builder_started: bool,
  lowest_free: Option<(PathBuf, u64)>,
  error_message: &mut String,
) -> circus_proto::BuildOutcome {
  match status {
    BuildStatus::Built
    | BuildStatus::Substituted
    | BuildStatus::AlreadyValid
    | BuildStatus::ResolvesToAlreadyValid => {
      circus_proto::BuildOutcome::Success
    },
    BuildStatus::PermanentFailure
    | BuildStatus::OutputRejected
    | BuildStatus::CachedFailure
    | BuildStatus::LogLimitExceeded
    | BuildStatus::NotDeterministic => circus_proto::BuildOutcome::BuildFailure,
    BuildStatus::TimedOut => circus_proto::BuildOutcome::TimedOut,
    BuildStatus::TransientFailure => {
      match lowest_free {
        Some((path, free)) if free < MIN_FREE_BYTES => {
          let reason = format!(
            "{} fell to {} MiB free during the build",
            path.display(),
            free >> 20
          );
          if !error_message.is_empty() {
            error_message.push('\n');
          }
          error_message.push_str(&reason);
          circus_proto::BuildOutcome::InfraFailure
        },
        _ if builder_started => circus_proto::BuildOutcome::TransientFailure,
        _ => circus_proto::BuildOutcome::InfraFailure,
      }
    },
    BuildStatus::InputRejected
    | BuildStatus::DependencyFailed
    | BuildStatus::MiscFailure
    | BuildStatus::NoSubstituters => circus_proto::BuildOutcome::InfraFailure,
  }
}

fn remember(recent: &mut VecDeque<String>, line: String) {
  if recent.len() == 32 {
    recent.pop_front();
  }
  recent.push_back(line);
}

/// Nix reports a build that filled the disk only in prose, and deletes the
/// build directory before exiting, so free space is sampled while it runs.
const MIN_FREE_BYTES: u64 = 1 << 30;
const DISK_SAMPLE_INTERVAL: Duration = Duration::from_secs(2);

/// The store, build, or temp directory with the least free space.
#[allow(
  clippy::useless_conversion,
  reason = "statvfs block counts are u32 on Darwin"
)]
fn lowest_free_space() -> Option<(PathBuf, u64)> {
  [
    PathBuf::from("/nix/store"),
    PathBuf::from("/nix/var/nix/builds"),
    std::env::temp_dir(),
  ]
  .into_iter()
  .filter_map(|path| {
    let stat = statvfs(&path).ok()?;
    let free =
      u64::from(stat.blocks_available()) * u64::from(stat.fragment_size());
    Some((path, free))
  })
  .min_by_key(|(_, free)| *free)
}

fn lower_free_space(
  seen: Option<(PathBuf, u64)>,
  sample: Option<(PathBuf, u64)>,
) -> Option<(PathBuf, u64)> {
  [seen, sample]
    .into_iter()
    .flatten()
    .min_by_key(|(_, free)| *free)
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

/// Nix reports a builder killed by a signal only as message text, so a build
/// that fails while the kernel OOM killer fires is taken to be its victim.
fn kernel_oom_kills() -> Option<u64> {
  std::fs::read_to_string("/proc/vmstat")
    .ok()?
    .lines()
    .find_map(|line| line.strip_prefix("oom_kill ")?.parse().ok())
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
  use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};

  use super::*;
  use crate::daemon::{STDERR_LAST, STDERR_START_ACTIVITY};

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
      .stdin(Stdio::piped())
      .stdout(Stdio::null())
      .stderr(Stdio::piped())
      .kill_on_drop(true);
    cmd
  }

  #[derive(Clone, Copy)]
  enum Answer {
    Hangup,
    Stall,
    Verdict {
      status:          u64,
      builder_started: bool,
    },
  }

  const DRV: &str = "/nix/store/00000000000000000000000000000000-test.drv";
  const OUT: &str = "/nix/store/00000000000000000000000000000000-test";

  async fn put_string(io: &mut DuplexStream, value: &str) {
    let len = value.len() as u64;
    io.write_u64_le(len).await.expect("write");
    io.write_all(value.as_bytes()).await.expect("write");
    let pad = (len.next_multiple_of(8) - len) as usize;
    io.write_all(&[0; 8][..pad]).await.expect("write");
  }

  async fn get_string(io: &mut DuplexStream) -> String {
    let len = io.read_u64_le().await.expect("read");
    let mut bytes = vec![0; len.next_multiple_of(8) as usize];
    io.read_exact(&mut bytes).await.expect("read");
    bytes.truncate(len as usize);
    String::from_utf8(bytes).expect("utf8")
  }

  /// The server side of the handshake and of one build, as Nix 2.34 sends it.
  async fn fake_daemon(mut io: DuplexStream, answer: Answer) {
    assert_eq!(io.read_u64_le().await.expect("magic"), 0x6E69_7863);
    if matches!(answer, Answer::Hangup) {
      return;
    }
    io.read_u64_le().await.expect("version");
    io.write_u64_le(0x6478_696F).await.expect("write");
    io.write_u64_le(0x126).await.expect("write");
    assert_eq!(io.read_u64_le().await.expect("features"), 0);
    io.write_u64_le(0).await.expect("write");
    io.read_u64_le().await.expect("affinity");
    io.read_u64_le().await.expect("reserve space");
    put_string(&mut io, "2.34.8").await;
    io.write_u64_le(1).await.expect("write");
    io.write_u64_le(STDERR_LAST).await.expect("write");

    assert_eq!(io.read_u64_le().await.expect("op"), 46);
    assert_eq!(io.read_u64_le().await.expect("count"), 1);
    assert_eq!(get_string(&mut io).await, format!("{DRV}!*"));
    io.read_u64_le().await.expect("mode");
    let Answer::Verdict {
      status,
      builder_started,
    } = answer
    else {
      std::future::pending::<()>().await;
      return;
    };

    if builder_started {
      io.write_u64_le(STDERR_START_ACTIVITY).await.expect("write");
      for value in [1, 3, 105] {
        io.write_u64_le(value).await.expect("write");
      }
      put_string(&mut io, "building").await;
      io.write_u64_le(0).await.expect("write");
      io.write_u64_le(0).await.expect("write");
    }
    io.write_u64_le(STDERR_LAST).await.expect("write");
    io.write_u64_le(1).await.expect("write");
    put_string(&mut io, &format!("{DRV}!*")).await;
    io.write_u64_le(status).await.expect("write");
    put_string(&mut io, if status == 0 { "" } else { "builder failed" }).await;
    for value in [1, 0, 0, 0, 0, 0, 0] {
      io.write_u64_le(value).await.expect("write");
    }

    if status == 0 {
      assert_eq!(io.read_u64_le().await.expect("op"), 41);
      assert_eq!(get_string(&mut io).await, DRV);
      io.write_u64_le(STDERR_LAST).await.expect("write");
      io.write_u64_le(1).await.expect("write");
      put_string(&mut io, "out").await;
      put_string(&mut io, OUT).await;
    }
    let _ = io.read_u64_le().await;
  }

  /// `child` stands in for the daemon process and supplies its stderr.
  #[expect(
    clippy::future_not_send,
    reason = "capnp futures are not Send; agent uses a single-threaded runtime"
  )]
  async fn build_with(
    child: &str,
    answer: Answer,
    opts: &BuildOptions<'_>,
    sink: log_sink::Client,
  ) -> LocalResult {
    let (client, server) = tokio::io::duplex(1 << 16);
    tokio::spawn(fake_daemon(server, answer));
    let (reader, writer) = tokio::io::split(client);
    let child = sh(child).spawn().expect("spawn");
    supervise(
      child,
      reader,
      writer,
      BuildMode::Normal,
      opts,
      fast_tunables(),
      sink,
      CancellationToken::new(),
    )
    .await
    .expect("supervise")
  }

  const SUCCESS: Answer = Answer::Verdict {
    status:          0,
    builder_started: true,
  };

  fn opts(max_log_size: u64, build_timeout: Duration) -> BuildOptions<'static> {
    BuildOptions {
      drv_path: DRV,
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
      let result = build_with(
        "seq 20000 >&2; cat >/dev/null",
        Answer::Verdict {
          status:          0,
          builder_started: false,
        },
        &opts(u64::MAX, Duration::from_mins(1)),
        h.sink,
      )
      .await;

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
      // The build finishes before the first write wedges.
      let h = harness(Some(0));
      let result = build_with(
        "seq 100 >&2; cat >/dev/null",
        SUCCESS,
        &opts(u64::MAX, Duration::from_mins(1)),
        h.sink,
      )
      .await;

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
  fn build_timeout_hangs_up_on_the_daemon() {
    run_local(async {
      let h = harness(None);
      let result = build_with(
        "cat >/dev/null",
        Answer::Stall,
        &opts(u64::MAX, Duration::from_millis(300)),
        h.sink,
      )
      .await;

      assert_eq!(result.outcome, circus_proto::BuildOutcome::TimedOut);
      assert_eq!(result.exit_code, -1);
      assert!(result.error_message.contains("build-timeout"));
    });
  }

  #[test]
  fn nix_verdicts_decide_who_failed() {
    run_local(async {
      for (answer, outcome) in [
        (SUCCESS, circus_proto::BuildOutcome::Success),
        (
          Answer::Verdict {
            status:          3,
            builder_started: true,
          },
          circus_proto::BuildOutcome::BuildFailure,
        ),
        (
          Answer::Verdict {
            status:          6,
            builder_started: true,
          },
          circus_proto::BuildOutcome::TransientFailure,
        ),
        (
          Answer::Verdict {
            status:          6,
            builder_started: false,
          },
          circus_proto::BuildOutcome::InfraFailure,
        ),
        (
          Answer::Verdict {
            status:          9,
            builder_started: true,
          },
          circus_proto::BuildOutcome::InfraFailure,
        ),
        (
          Answer::Verdict {
            status:          4,
            builder_started: false,
          },
          circus_proto::BuildOutcome::InfraFailure,
        ),
        (
          Answer::Verdict {
            status:          10,
            builder_started: false,
          },
          circus_proto::BuildOutcome::InfraFailure,
        ),
        (Answer::Hangup, circus_proto::BuildOutcome::InfraFailure),
      ] {
        let h = harness(None);
        let result = build_with(
          "cat >/dev/null",
          answer,
          &opts(1 << 20, Duration::from_mins(1)),
          h.sink,
        )
        .await;

        assert_eq!(result.outcome, outcome);
        if outcome == circus_proto::BuildOutcome::Success {
          assert_eq!(result.outputs.len(), 1);
          assert_eq!(result.outputs[0].path, OUT);
        }
      }
    });
  }

  #[test]
  fn max_log_size_fails_running_build() {
    run_local(async {
      let h = harness(None);
      let result = build_with(
        "seq 10000 >&2; cat >/dev/null",
        Answer::Stall,
        &opts(64, Duration::from_mins(1)),
        h.sink,
      )
      .await;

      assert_eq!(result.outcome, circus_proto::BuildOutcome::BuildFailure);
      assert!(result.error_message.contains("max-log-size exceeded"));
    });
  }
}
