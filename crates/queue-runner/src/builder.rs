use std::{
  ffi::OsString,
  path::{Path, PathBuf},
  time::Duration,
};

use circus_common::{CiError, error::Result};
use tokio::{
  io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
  process::Command,
  task::JoinHandle,
};

const MAX_LOG_SIZE: usize = 100 * 1024 * 1024; // 100MB

pub struct BuildResult {
  pub success:              bool,
  pub exit_code:            Option<i32>,
  pub stdout:               String,
  pub stderr:               String,
  pub output_paths:         Vec<String>,
  pub cache_upload_handled: bool,
}

/// Run `nix build` for a derivation path.
/// If `live_log_path` is provided, build output is streamed to that file
/// incrementally.
///
/// # Errors
///
/// Returns error if nix build command fails or times out.
#[tracing::instrument(skip(work_dir, live_log_path), fields(drv_path))]
pub async fn run_nix_build(
  drv_path: &str,
  work_dir: &Path,
  timeout: Duration,
  live_log_path: Option<&Path>,
  extra_args: &[String],
) -> Result<BuildResult> {
  run_nix_build_command(
    common_nix_build_args(drv_path, extra_args),
    work_dir,
    timeout,
    live_log_path,
    "nix build",
    |_| {},
  )
  .await
}

fn common_nix_build_args(drv_path: &str, extra: &[String]) -> Vec<OsString> {
  // Build every output of the derivation via the "^*" selector. A bare ".drv"
  // path is treated by `nix build` as a store path to realise (a no-op that
  // emits the .drv path rather than building it), so the outputs would never
  // be produced and --print-out-paths would not emit real output paths.
  let installable = format!("{drv_path}^*");
  let defaults: [&str; 11] = [
    "build",
    "--no-link",
    "--print-out-paths",
    "--log-format",
    "internal-json",
    "--option",
    "sandbox",
    "true",
    "--max-build-log-size",
    "104857600",
    installable.as_str(),
  ];
  // Operator-supplied args go before the installable so they remain valid
  // overrides (nix's `--option` resolution lets later flags win), while the
  // installable stays in last position where `nix build` requires it.
  let mut args: Vec<OsString> =
    Vec::with_capacity(defaults.len() + extra.len());
  for s in &defaults[..defaults.len() - 1] {
    args.push(OsString::from(*s));
  }
  for s in extra {
    args.push(OsString::from(s));
  }
  args.push(OsString::from(installable));
  args
}

async fn run_nix_build_command(
  args: Vec<OsString>,
  work_dir: &Path,
  timeout: Duration,
  live_log_path: Option<&Path>,
  operation: &'static str,
  configure: impl FnOnce(&mut Command),
) -> Result<BuildResult> {
  let result = tokio::time::timeout(timeout, async {
    let mut cmd = Command::new("nix");
    cmd
      .args(args)
      .current_dir(work_dir)
      .kill_on_drop(true)
      .stdout(std::process::Stdio::piped())
      .stderr(std::process::Stdio::piped());
    configure(&mut cmd);

    let mut child = cmd
      .spawn()
      .map_err(|e| CiError::Build(format!("Failed to run {operation}: {e}")))?;

    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();

    let stdout_task = read_stdout(stdout_handle);
    let stderr_task =
      read_stderr(stderr_handle, live_log_path.map(Path::to_path_buf));

    let stdout_buf = join_output(stdout_task, "stdout reader").await?;
    let stderr_buf = join_output(stderr_task, "stderr reader").await?;

    let status = child.wait().await.map_err(|e| {
      CiError::Build(format!("Failed to wait for {operation}: {e}"))
    })?;

    let output_paths: Vec<String> = stdout_buf
      .lines()
      .map(|s| s.trim().to_string())
      .filter(|s| !s.is_empty())
      .collect();

    Ok::<_, CiError>(BuildResult {
      success: status.success(),
      exit_code: status.code(),
      stdout: stdout_buf,
      stderr: stderr_buf,
      output_paths,
      cache_upload_handled: false,
    })
  })
  .await;

  result.unwrap_or_else(|_| {
    Err(CiError::Timeout(format!(
      "{operation} timed out after {timeout:?}"
    )))
  })
}

fn read_stdout(
  stdout: Option<tokio::process::ChildStdout>,
) -> JoinHandle<Result<String>> {
  tokio::spawn(async move {
    let mut buf = String::new();
    if let Some(stdout) = stdout {
      let mut reader = BufReader::new(stdout);
      let mut line = String::new();
      while reader.read_line(&mut line).await.map_err(|e| {
        CiError::Build(format!("Failed to read nix stdout: {e}"))
      })?
        > 0
      {
        buf.push_str(&line);
        line.clear();
      }
    }
    Ok(buf)
  })
}

fn read_stderr(
  stderr: Option<tokio::process::ChildStderr>,
  live_log_path: Option<PathBuf>,
) -> JoinHandle<Result<String>> {
  tokio::spawn(async move {
    let mut buf = String::new();
    let mut log_file = if let Some(ref path) = live_log_path {
      match tokio::fs::File::create(path).await {
        Ok(file) => Some(file),
        Err(e) => {
          tracing::warn!(
            path = %path.display(),
            "Failed to create live build log: {e}"
          );
          None
        },
      }
    } else {
      None
    };
    let mut logged_write_error = false;

    if let Some(stderr) = stderr {
      let mut reader = BufReader::new(stderr);
      let mut line = String::new();
      while reader.read_line(&mut line).await.map_err(|e| {
        CiError::Build(format!("Failed to read nix stderr: {e}"))
      })?
        > 0
      {
        if let Some(ref mut file) = log_file
          && let Err(e) = write_live_log_line(file, &line).await
          && !logged_write_error
        {
          tracing::warn!("Failed to write live build log: {e}");
          logged_write_error = true;
        }

        if buf.len() < MAX_LOG_SIZE {
          buf.push_str(&line);
        }
        line.clear();
      }
    }

    Ok(buf)
  })
}

async fn write_live_log_line(
  file: &mut tokio::fs::File,
  line: &str,
) -> std::io::Result<()> {
  file.write_all(line.as_bytes()).await?;
  file.flush().await
}

async fn join_output<T>(task: JoinHandle<Result<T>>, label: &str) -> Result<T> {
  task
    .await
    .map_err(|e| CiError::Build(format!("Nix {label} task failed: {e}")))?
}
