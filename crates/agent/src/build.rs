//! One-shot build executor. Spawns `nix-store --realise`, streams stdout
//! and stderr through a `LogSink`, and assembles a `BuildResult` at exit.
use std::{
  collections::BTreeMap,
  process::Stdio,
  time::{Duration, Instant},
};

use circus_proto::log_sink;
use tokio::{
  io::{AsyncBufReadExt, BufReader},
  process::Command,
  time::timeout,
};
use tokio_util::sync::CancellationToken;

/// Per-build options handed down from the runner via the capnp schema.
pub struct BuildOptions<'a> {
  pub drv_path:        &'a str,
  pub max_log_size:    u64,
  pub max_silent_time: Duration,
  pub build_timeout:   Duration,
  pub extra_args:      Vec<String>,
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
/// passed in via `Builder.assign`. We call `write(chunk)` for each log
/// line and `close()` at the end. Failures on the sink are logged at the
/// agent and ignored otherwise; the build still completes.
///
/// `cancel` is signalled by [`crate::session::BuilderImpl::abort`]. When
/// it fires, the child is SIGTERM'd and the function returns an aborted
/// outcome.
///
/// # Errors
///
/// Returns the failure as a `LocalResult` with a non-success outcome rather
/// than `Result::Err`. Only IO failures around spawning the child are
/// raised as anyhow errors.
pub async fn run(
  opts: BuildOptions<'_>,
  log_sink: log_sink::Client,
  cancel: CancellationToken,
) -> anyhow::Result<LocalResult> {
  let mut args: Vec<String> = vec![
    "--realise".into(),
    "--log-format".into(),
    "internal-json".into(),
    opts.drv_path.into(),
  ];
  args.extend(opts.extra_args.iter().cloned());

  let mut cmd = Command::new("nix-store");
  cmd
    .args(&args)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);

  let started = Instant::now();
  let mut child = cmd.spawn()?;
  let stdout = child
    .stdout
    .take()
    .ok_or_else(|| anyhow::anyhow!("child stdout missing"))?;
  let stderr = child
    .stderr
    .take()
    .ok_or_else(|| anyhow::anyhow!("child stderr missing"))?;

  // Drive stdout and stderr in two independent tasks so that EOF on one
  // stream does not cause lines buffered in the other to be discarded.
  // Both tasks forward lines (or IO errors) through a shared channel.
  let (line_tx, mut line_rx) =
    tokio::sync::mpsc::unbounded_channel::<Result<String, String>>();
  {
    let tx = line_tx.clone();
    let mut reader = BufReader::new(stdout).lines();
    tokio::spawn(async move {
      loop {
        match reader.next_line().await {
          Ok(Some(line)) => {
            if tx.send(Ok(line)).is_err() {
              break;
            }
          },
          Ok(None) => break,
          Err(e) => {
            let _ = tx.send(Err(format!("stdout read: {e}")));
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
            if tx.send(Ok(line)).is_err() {
              break;
            }
          },
          Ok(None) => break,
          Err(e) => {
            let _ = tx.send(Err(format!("stderr read: {e}")));
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
  let mut aborted = false;

  let overall_deadline = if opts.build_timeout.is_zero() {
    None
  } else {
    Some(Instant::now() + opts.build_timeout)
  };
  let mut last_output = Instant::now();

  loop {
    let read_timeout = remaining_silent(&opts.max_silent_time, last_output);
    let msg: Option<Result<String, String>> = tokio::select! {
      r = line_rx.recv() => r,
      () = sleep_opt(read_timeout) => {
        error_message = "max-silent-time exceeded".into();
        let _ = child.start_kill();
        break;
      }
      () = cancel.cancelled() => {
        aborted = true;
        error_message = "aborted by runner".into();
        let _ = child.start_kill();
        break;
      }
    };

    let line = match msg {
      None => break,
      Some(Err(e)) => {
        error_message = e;
        break;
      },
      Some(Ok(l)) => l,
    };
    last_output = Instant::now();

    if log_size_exceeded {
      continue;
    }
    if bytes_sent.saturating_add(line.len() as u64) > opts.max_log_size {
      log_size_exceeded = true;
      error_message = "max-log-size exceeded".into();
      let _ = child.start_kill();
      continue;
    }
    bytes_sent = bytes_sent.saturating_add(line.len() as u64 + 1);
    if let Err(e) = forward_chunk(&log_sink, line.as_bytes()).await {
      tracing::warn!(error = ?e, "log sink write failed; killing child");
      log_size_exceeded = true;
      let _ = child.start_kill();
    }

    if let Some(deadline) = overall_deadline
      && Instant::now() >= deadline
    {
      error_message = "build-timeout exceeded".into();
      let _ = child.start_kill();
      break;
    }
  }

  let status = match overall_deadline {
    Some(deadline) => {
      match timeout(
        deadline.saturating_duration_since(Instant::now()),
        child.wait(),
      )
      .await
      {
        Ok(s) => s?,
        Err(_) => {
          let _ = child.kill().await;
          let _ = close_log(&log_sink).await;
          return Ok(LocalResult {
            outcome:        circus_proto::BuildOutcome::TimedOut,
            exit_code:      -1,
            build_time_ms:  started.elapsed().as_millis() as u64,
            upload_time_ms: 0,
            outputs:        Vec::new(),
            error_message:  "build-timeout exceeded".into(),
          });
        },
      }
    },
    None => child.wait().await?,
  };

  let _ = close_log(&log_sink).await;

  let exit_code = status.code().unwrap_or(-1);
  let success = status.success() && !log_size_exceeded && !aborted;
  let outcome = if success {
    circus_proto::BuildOutcome::Success
  } else if aborted {
    circus_proto::BuildOutcome::Aborted
  } else {
    circus_proto::BuildOutcome::BuildFailure
  };

  let outputs = if success {
    query_outputs(opts.drv_path).await
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
async fn query_outputs(drv_path: &str) -> Vec<ResolvedOutput> {
  if let Some(parsed) = query_outputs_via_show(drv_path).await {
    return parsed;
  }
  match tokio::process::Command::new("nix-store")
    .args(["--query", "--outputs", drv_path])
    .output()
    .await
  {
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

async fn query_outputs_via_show(drv_path: &str) -> Option<Vec<ResolvedOutput>> {
  let out = tokio::process::Command::new("nix")
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
  let mut req = sink.write_request();
  req.get().set_chunk(chunk);
  req.send().promise.await?;
  Ok(())
}

async fn close_log(sink: &log_sink::Client) -> Result<(), capnp::Error> {
  sink.close_request().send().promise.await?;
  Ok(())
}
