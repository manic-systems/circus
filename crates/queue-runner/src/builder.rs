use std::{path::Path, time::Duration};

use circus_common::{CiError, error::Result};
use tokio::io::{AsyncBufReadExt, BufReader};

const MAX_LOG_SIZE: usize = 100 * 1024 * 1024; // 100MB

/// Run a build on a remote machine via `nix build --store ssh://...`.
#[tracing::instrument(
  skip(work_dir, live_log_path),
  fields(drv_path, store_uri)
)]
/// Run a nix build on a remote builder via SSH.
///
/// # Errors
///
/// Returns error if nix build command fails or times out.
pub async fn run_nix_build_remote(
  drv_path: &str,
  work_dir: &Path,
  timeout: Duration,
  store_uri: &str,
  ssh_key_file: Option<&str>,
  live_log_path: Option<&Path>,
) -> Result<BuildResult> {
  let result = tokio::time::timeout(timeout, async {
    let mut cmd = tokio::process::Command::new("nix");
    cmd
      .args([
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
        "--store",
        store_uri,
        drv_path,
      ])
      .current_dir(work_dir)
      .kill_on_drop(true)
      .stdout(std::process::Stdio::piped())
      .stderr(std::process::Stdio::piped());

    if let Some(key_file) = ssh_key_file {
      cmd.env(
        "NIX_SSHOPTS",
        format!("-i {key_file} -o StrictHostKeyChecking=accept-new"),
      );
    }

    let mut child = cmd.spawn().map_err(|e| {
      CiError::Build(format!("Failed to run remote nix build: {e}"))
    })?;

    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();

    let stdout_task = tokio::spawn(async move {
      let mut buf = String::new();
      if let Some(stdout) = stdout_handle {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
          buf.push_str(&line);
          line.clear();
        }
      }
      buf
    });

    let live_log_path_owned = live_log_path.map(std::path::Path::to_path_buf);
    let stderr_task = tokio::spawn(async move {
      let mut buf = String::new();
      let steps: Vec<SubStep> = Vec::new();
      let mut log_file = if let Some(ref path) = live_log_path_owned {
        tokio::fs::File::create(path).await.ok()
      } else {
        None
      };

      if let Some(stderr) = stderr_handle {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
          if let Some(ref mut f) = log_file {
            use tokio::io::AsyncWriteExt;
            let _ = f.write_all(line.as_bytes()).await;
            let _ = f.flush().await;
          }
          if buf.len() < MAX_LOG_SIZE {
            buf.push_str(&line);
          }
          line.clear();
        }
      }
      (buf, steps)
    });

    let stdout_buf = stdout_task.await.unwrap_or_default();
    let (stderr_buf, sub_steps) = stderr_task.await.unwrap_or_default();

    let status = child.wait().await.map_err(|e| {
      CiError::Build(format!("Failed to wait for remote nix build: {e}"))
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
      sub_steps,
    })
  })
  .await;

  result.unwrap_or_else(|_| {
    Err(CiError::Timeout(format!(
      "Remote build timed out after {timeout:?}"
    )))
  })
}

pub struct BuildResult {
  pub success:      bool,
  pub exit_code:    Option<i32>,
  pub stdout:       String,
  pub stderr:       String,
  pub output_paths: Vec<String>,
  pub sub_steps:    Vec<SubStep>,
}

/// A sub-step parsed from nix's internal JSON log format.
pub struct SubStep {
  pub drv_path:     String,
  pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
  pub success:      bool,
}

/// Parse a single nix internal JSON log line (`@nix {...}`).
/// Returns `Some(action, drv_path)` if the line contains a derivation action.
#[must_use]
pub fn parse_nix_log_line(line: &str) -> Option<(&'static str, String)> {
  let json_str = line.strip_prefix("@nix ")?.trim();
  let parsed: serde_json::Value = serde_json::from_str(json_str).ok()?;
  let action = parsed.get("action")?.as_str()?;
  let drv = parsed.get("derivation")?.as_str()?.to_string();

  match action {
    "start" => Some(("start", drv)),
    "stop" => Some(("stop", drv)),
    _ => None,
  }
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
) -> Result<BuildResult> {
  let result = tokio::time::timeout(timeout, async {
    let mut child = tokio::process::Command::new("nix")
      .args([
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
        drv_path,
      ])
      .current_dir(work_dir)
      .kill_on_drop(true)
      .stdout(std::process::Stdio::piped())
      .stderr(std::process::Stdio::piped())
      .spawn()
      .map_err(|e| CiError::Build(format!("Failed to run nix build: {e}")))?;

    let stdout_handle = child.stdout.take();
    let stderr_handle = child.stderr.take();

    // Read stdout (output paths)
    let stdout_task = tokio::spawn(async move {
      let mut buf = String::new();
      if let Some(stdout) = stdout_handle {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
          buf.push_str(&line);
          line.clear();
        }
      }
      buf
    });

    // Read stderr (logs + internal JSON)
    let live_log_path_owned = live_log_path.map(std::path::Path::to_path_buf);
    let stderr_task = tokio::spawn(async move {
      let mut buf = String::new();
      let mut steps: Vec<SubStep> = Vec::new();
      let mut log_file = if let Some(ref path) = live_log_path_owned {
        tokio::fs::File::create(path).await.ok()
      } else {
        None
      };

      if let Some(stderr) = stderr_handle {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        while reader.read_line(&mut line).await.unwrap_or(0) > 0 {
          // Write to live log file if available
          if let Some(ref mut f) = log_file {
            use tokio::io::AsyncWriteExt;
            let _ = f.write_all(line.as_bytes()).await;
            let _ = f.flush().await;
          }

          // Parse nix internal JSON log lines
          if line.starts_with("@nix ")
            && let Some(json_str) = line.strip_prefix("@nix ")
            && let Ok(parsed) =
              serde_json::from_str::<serde_json::Value>(json_str.trim())
            && let Some(action) = parsed.get("action").and_then(|a| a.as_str())
          {
            match action {
              "start" => {
                if let Some(drv) =
                  parsed.get("derivation").and_then(|d| d.as_str())
                {
                  steps.push(SubStep {
                    drv_path:     drv.to_string(),
                    completed_at: None,
                    success:      false,
                  });
                }
              },
              "stop" => {
                if let Some(drv) =
                  parsed.get("derivation").and_then(|d| d.as_str())
                  && let Some(step) =
                    steps.iter_mut().rfind(|s| s.drv_path == drv)
                {
                  step.completed_at = Some(chrono::Utc::now());
                  step.success = true;
                }
              },
              _ => {},
            }
          }

          if buf.len() < MAX_LOG_SIZE {
            buf.push_str(&line);
          }
          line.clear();
        }
      }
      (buf, steps)
    });

    let stdout_buf = stdout_task.await.unwrap_or_default();
    let (stderr_buf, sub_steps) = stderr_task.await.unwrap_or_default();

    let status = child.wait().await.map_err(|e| {
      CiError::Build(format!("Failed to wait for nix build: {e}"))
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
      sub_steps,
    })
  })
  .await;

  result.unwrap_or_else(|_| {
    Err(CiError::Timeout(format!(
      "Build timed out after {timeout:?}"
    )))
  })
}
