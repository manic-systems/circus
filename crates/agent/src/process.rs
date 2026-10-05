//! Supervises an effect's process, streaming its stdout and stderr through a
//! `LogSink` and killing its whole process group on abort or timeout.
use std::{
  collections::VecDeque,
  io,
  os::unix::process::ExitStatusExt,
  process::ExitStatus,
  time::{Duration, Instant},
};

use circus_proto::{log_sink, nix_log};
use nix::{
  sys::signal::{Signal, killpg},
  unistd::{Pid, getpgid},
};
use tokio::{
  io::{AsyncBufReadExt, BufReader},
  process::{Child, Command},
  time::timeout,
};
use tokio_util::sync::CancellationToken;

use crate::build::{
  LOG_CHANNEL_CAPACITY,
  LocalResult,
  MAX_LOG_BATCH_BYTES,
  MAX_LOG_BATCH_LINES,
  Tunables,
  close_log,
  forward_chunk,
  kernel_oom_kills,
  remaining_silent,
  sleep_opt,
  summarize_failure,
};

pub struct CommandOptions {
  pub max_log_size:    u64,
  pub max_silent_time: Duration,
  pub build_timeout:   Duration,
  /// Secrets replaced with `[REDACTED]` before a line reaches the log.
  pub redactions:      Vec<String>,
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
pub async fn run_command(
  mut cmd: Command,
  opts: &CommandOptions,
  tun: Tunables,
  log_sink: log_sink::Client,
  cancel: CancellationToken,
) -> color_eyre::Result<LocalResult> {
  let started = Instant::now();
  let oom_kills_before = kernel_oom_kills();
  let mut child = cmd.spawn()?;
  let group = child
    .id()
    .and_then(|pid| i32::try_from(pid).ok())
    .map(Pid::from_raw)
    .filter(|pid| getpgid(Some(*pid)) == Ok(*pid));
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
        kill_child(&mut child, group);
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
          kill_child(&mut child, group);
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
        kill_child(&mut child, group);
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
      let line = redact_line(&line, &opts.redactions);
      if recent_msgs.len() == 32 {
        recent_msgs.pop_front();
      }
      recent_msgs.push_back(line.clone());

      if !log_size_exceeded
        && !log_truncated
        && !sink_failed
        && !nix_log::is_progress(&line)
      {
        if bytes_sent.saturating_add(line.len() as u64 + 1) > opts.max_log_size
        {
          if child_status.is_none() {
            log_size_exceeded = true;
            error_message = "max-log-size exceeded".into();
            kill_child(&mut child, group);
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
          kill_child(&mut child, group);
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
        kill_child(&mut child, group);
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
        kill_child(&mut child, group);
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
        kill_group(group);
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
            kill_group(group);
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
  kill_group(group);

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
  let oom_killed = !killed
    && !aborted
    && !timed_out
    && (matches!(status.signal(), Some(9))
      || !status.success()
        && oom_kills_before
          .zip(kernel_oom_kills())
          .is_some_and(|(before, after)| after > before));
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

  Ok(LocalResult {
    outcome,
    exit_code,
    build_time_ms: started.elapsed().as_millis() as u64,
    upload_time_ms: 0,
    outputs: Vec::new(),
    error_message,
  })
}

fn redact_line(line: &str, secrets: &[String]) -> String {
  secrets.iter().fold(line.to_owned(), |redacted, secret| {
    if secret.is_empty() || !redacted.contains(secret) {
      redacted
    } else {
      redacted.replace(secret, "[REDACTED]")
    }
  })
}

/// Send SIGKILL to the process group when present.
fn kill_group(group: Option<Pid>) {
  if let Some(group) = group {
    let _ = killpg(group, Signal::SIGKILL);
  }
}

fn kill_child(child: &mut Child, group: Option<Pid>) {
  kill_group(group);
  let _ = child.start_kill();
}

#[cfg(test)]
mod tests {
  use std::{cell::RefCell, future::Future, process::Stdio, rc::Rc};

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

  fn opts(max_log_size: u64, build_timeout: Duration) -> CommandOptions {
    CommandOptions {
      max_log_size,
      max_silent_time: Duration::ZERO,
      build_timeout,
      redactions: Vec::new(),
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

  #[test]
  fn secret_values_are_redacted_from_logs() {
    run_local(async {
      let h = harness(None);
      let mut options = opts(u64::MAX, Duration::from_mins(1));
      options.redactions = vec!["top-secret-value".into()];
      let result = run_command(
        sh("printf 'safe-1\\ntoken=top-secret-value\\nsafe-3\\n'"),
        &options,
        fast_tunables(),
        h.sink,
        CancellationToken::new(),
      )
      .await
      .expect("run_command");

      assert_eq!(result.outcome, circus_proto::BuildOutcome::Success);
      let chunks = h.chunks.borrow();
      let joined = chunks
        .iter()
        .map(|chunk| String::from_utf8_lossy(chunk))
        .collect::<Vec<_>>()
        .join("\n");
      assert!(joined.contains("token=[REDACTED]"));
      assert!(!joined.contains("top-secret-value"));
    });
  }
}
