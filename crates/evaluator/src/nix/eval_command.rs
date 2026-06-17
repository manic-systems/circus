use std::time::Duration;

use circus_common::{CiError, error::Result};
use circus_config::EvaluatorConfig;
use tokio::process::Command;

use super::{EvalResult, parse_eval_output};

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
          Err(CiError::NixEval("Nix evaluation failed".to_string()))
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
