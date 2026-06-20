use std::time::Duration;

use circus_common::{CiError, error::Result};
use circus_config::EvaluatorConfig;
use tokio::process::Command;

use super::{EvalResult, parse_eval_output};

/// Maximum number of stderr bytes to retain in a surfaced evaluation error.
/// nix prints the actual failure at the tail, so we keep the end of the output.
const MAX_STDERR_BYTES: usize = 4096;
/// Maximum number of trailing stderr lines to retain.
const MAX_STDERR_LINES: usize = 40;

/// Distil nix-eval-jobs stderr into a bounded, human-readable detail string
/// suitable for storing as an evaluation error and showing on the dashboard.
///
/// Keeps the tail of the output (where nix reports the actual error) and caps
/// both line count and byte length so a runaway trace can't bloat the database
/// or the rendered page.
fn summarize_stderr(stderr: &str) -> String {
  let trimmed = stderr.trim();
  if trimmed.is_empty() {
    return String::new();
  }

  let lines: Vec<&str> = trimmed.lines().collect();
  let tail = if lines.len() > MAX_STDERR_LINES {
    &lines[lines.len() - MAX_STDERR_LINES..]
  } else {
    &lines[..]
  };
  let mut detail = tail.join("\n");

  if detail.len() > MAX_STDERR_BYTES {
    // Keep the tail; truncate on a char boundary to stay valid UTF-8.
    let start = detail.len() - MAX_STDERR_BYTES;
    let start = (start..detail.len())
      .find(|&i| detail.is_char_boundary(i))
      .unwrap_or(detail.len());
    detail = detail[start..].to_string();
  }

  detail
}

pub(super) struct NixEvalPolicy {
  restrict_eval: bool,
  allow_ifd:     bool,
}

impl NixEvalPolicy {
  pub(super) fn apply_to(&self, cmd: &mut Command) {
    if self.restrict_eval {
      cmd.args(["--option", "restrict-eval", "true"]);
    }
    if !self.allow_ifd {
      cmd.args(["--option", "allow-import-from-derivation", "false"]);
    }
  }
}

impl From<&EvaluatorConfig> for NixEvalPolicy {
  fn from(config: &EvaluatorConfig) -> Self {
    Self {
      restrict_eval: config.restrict_eval,
      allow_ifd:     config.allow_ifd,
    }
  }
}

pub(super) struct EvalCommand {
  cmd:         Command,
  timeout:     Duration,
  description: &'static str,
}

impl EvalCommand {
  pub(super) const fn new(
    cmd: Command,
    timeout: Duration,
    description: &'static str,
  ) -> Self {
    Self {
      cmd,
      timeout,
      description,
    }
  }

  pub(super) async fn run(mut self) -> Result<EvalResult> {
    let timeout = self.timeout;
    let description = self.description;

    tokio::time::timeout(timeout, async move {
      let output = self.cmd.output().await;

      match output {
        Ok(out) if out.status.success() || !out.stdout.is_empty() => {
          let stdout = String::from_utf8_lossy(&out.stdout);
          let result = parse_eval_output(&stdout);

          if result.error_count > 0 {
            tracing::warn!(
              error_count = result.error_count,
              "{description} nix-eval-jobs reported errors for some jobs"
            );
          }

          if result.jobs.is_empty() && result.error_count == 0 {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !stderr.trim().is_empty() {
              tracing::warn!(
                stderr = %stderr,
                "{description} nix-eval-jobs returned no jobs, stderr output present"
              );
            }
          }

          Ok(result)
        },
        Ok(out) => {
          let stderr = String::from_utf8_lossy(&out.stderr);
          tracing::warn!(stderr = %stderr, "{description} nix-eval-jobs failed");
          let detail = summarize_stderr(&stderr);
          let message = if detail.is_empty() {
            format!(
              "nix-eval-jobs exited with {} and produced no output",
              out.status
            )
          } else {
            format!("nix-eval-jobs failed ({}):\n{detail}", out.status)
          };
          Err(CiError::NixEval(message))
        },
        Err(e) => Err(CiError::NixEval(format!(
          "Failed to run nix-eval-jobs: {e}"
        ))),
      }
    })
    .await
    .map_err(|_| {
      CiError::Timeout(format!("Nix evaluation timed out after {timeout:?}"))
    })?
  }
}
