use std::{
  sync::{
    Arc,
    Mutex,
    atomic::{AtomicBool, Ordering},
  },
  time::Duration,
};

use circus_common::{CiError, error::Result};
use circus_config::EvaluatorConfig;
use tokio::process::Command;

use super::{EvalResult, NixJob, nix_job_from_derivation};

/// Nix evaluation settings derived from the evaluator configuration, forwarded
/// to evix as `(key, value)` options (which evix applies via `NIX_CONFIG`).
pub(super) struct NixEvalPolicy {
  restrict_eval: bool,
  allow_ifd:     bool,
  allowed_uris:  Vec<String>,
}

impl NixEvalPolicy {
  pub(super) fn nix_options(&self) -> Vec<(String, String)> {
    let mut options = Vec::new();
    if self.restrict_eval {
      options.push(("restrict-eval".to_string(), "true".to_string()));
    }
    if !self.allow_ifd {
      options.push((
        "allow-import-from-derivation".to_string(),
        "false".to_string(),
      ));
    }
    if !self.allowed_uris.is_empty() {
      options.push(("allowed-uris".to_string(), self.allowed_uris.join(" ")));
    }
    options
  }

  /// Apply the policy to a `nix` CLI invocation as `--option KEY VALUE` flags.
  /// Used for the auxiliary `nix eval` / `nix derivation show` calls that back
  /// `nixosConfigurations` resolution.
  pub(super) fn apply_to(&self, cmd: &mut Command) {
    if self.restrict_eval {
      cmd.args(["--option", "restrict-eval", "true"]);
    }
    if !self.allow_ifd {
      cmd.args(["--option", "allow-import-from-derivation", "false"]);
    }
    if !self.allowed_uris.is_empty() {
      cmd.args(["--option", "allowed-uris", &self.allowed_uris.join(" ")]);
    }
  }
}

impl From<&EvaluatorConfig> for NixEvalPolicy {
  fn from(config: &EvaluatorConfig) -> Self {
    Self {
      restrict_eval: config.restrict_eval,
      allow_ifd:     config.allow_ifd,
      allowed_uris:  config.allowed_uris.clone(),
    }
  }
}

/// Drive an evix evaluation to completion, collecting jobs and per-job errors.
///
/// evix exposes a blocking API backed by worker subprocesses, so it runs on a
/// blocking task. `timeout` is enforced by setting evix's cancellation flag,
/// which winds the workers down rather than leaking them.
///
/// # Returns
///
/// The discovered [`NixJob`]s and the count of jobs that failed to evaluate
/// individually (non-fatal per-attribute errors).
///
/// # Errors
///
/// [`CiError::Timeout`] if evaluation exceeds `timeout`, or
/// [`CiError::NixEval`] if the evaluation fails fatally or the blocking task
/// cannot be joined.
pub(super) async fn run_eval(
  config: evix::Config,
  timeout: Duration,
  description: &'static str,
) -> Result<EvalResult> {
  let collected: Arc<Mutex<(Vec<NixJob>, usize)>> =
    Arc::new(Mutex::new((Vec::new(), 0)));
  let cancel = Arc::new(AtomicBool::new(false));

  let sink_state = Arc::clone(&collected);
  let cancel_eval = Arc::clone(&cancel);
  let handle = tokio::task::spawn_blocking(move || {
    evix::evaluate_cancellable(&config, &cancel_eval, move |event| {
      match event {
        evix::Event::Derivation(drv) => {
          sink_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .0
            .push(nix_job_from_derivation(drv));
        },
        evix::Event::Error(err) => {
          tracing::warn!(job = %err.attr, "evix reported error: {}", err.error);
          sink_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .1 += 1;
        },
        evix::Event::AttrSet { .. } => {},
      }
      Ok(())
    })
  });

  match tokio::time::timeout(timeout, handle).await {
    Ok(joined) => {
      joined
        .map_err(|e| {
          CiError::NixEval(format!("evix evaluation task failed to join: {e}"))
        })?
        .map_err(|e| {
          CiError::NixEval(format!(
            "{description} evix evaluation failed: {e:#}"
          ))
        })?;

      let result = {
        let guard = collected
          .lock()
          .unwrap_or_else(std::sync::PoisonError::into_inner);
        EvalResult {
          jobs:        guard.0.clone(),
          error_count: guard.1,
        }
      };
      if result.error_count > 0 {
        tracing::warn!(
          error_count = result.error_count,
          "{description} evix reported errors for some jobs"
        );
      }
      if result.jobs.is_empty() && result.error_count == 0 {
        tracing::warn!("{description} evix returned no jobs");
      }
      Ok(result)
    },
    Err(_elapsed) => {
      // Ask the in-flight evaluation to stop; its workers exit cooperatively.
      cancel.store(true, Ordering::Relaxed);
      Err(CiError::Timeout(format!(
        "Nix evaluation timed out after {timeout:?}"
      )))
    },
  }
}

#[cfg(test)]
mod policy_tests {
  use super::*;

  fn policy(restrict_eval: bool, allow_ifd: bool) -> NixEvalPolicy {
    NixEvalPolicy {
      restrict_eval,
      allow_ifd,
      allowed_uris: Vec::new(),
    }
  }

  #[test]
  fn no_options_when_permissive() {
    assert!(policy(false, true).nix_options().is_empty());
  }

  #[test]
  fn allowed_uris_joined_with_space() {
    let p = NixEvalPolicy {
      restrict_eval: true,
      allow_ifd:     false,
      allowed_uris:  vec![
        "https://releases.nixos.org".to_string(),
        "https://github.com".to_string(),
      ],
    };
    let opts = p.nix_options();
    let (_, v) = opts
      .iter()
      .find(|(k, _)| k == "allowed-uris")
      .expect("allowed-uris option must be present");
    assert_eq!(v, "https://releases.nixos.org https://github.com");
  }

  #[test]
  fn empty_allowed_uris_emits_no_option() {
    let opts = policy(true, false).nix_options();
    assert!(!opts.iter().any(|(k, _)| k == "allowed-uris"));
  }

  #[test]
  fn restrict_eval_emits_option() {
    let opts = policy(true, true).nix_options();
    assert!(
      opts
        .iter()
        .any(|(k, v)| k == "restrict-eval" && v == "true")
    );
    assert!(
      !opts
        .iter()
        .any(|(k, _)| k == "allow-import-from-derivation")
    );
  }

  #[test]
  fn no_ifd_emits_option() {
    let opts = policy(false, false).nix_options();
    assert!(
      opts
        .iter()
        .any(|(k, v)| k == "allow-import-from-derivation" && v == "false")
    );
    assert!(!opts.iter().any(|(k, _)| k == "restrict-eval"));
  }

  #[test]
  fn both_flags_emit_both_options() {
    let opts = policy(true, false).nix_options();
    assert_eq!(opts.len(), 2);
    assert!(
      opts
        .iter()
        .any(|(k, v)| k == "restrict-eval" && v == "true")
    );
    assert!(
      opts
        .iter()
        .any(|(k, v)| k == "allow-import-from-derivation" && v == "false")
    );
  }

  #[test]
  fn from_evaluator_config() {
    let config = EvaluatorConfig {
      restrict_eval: true,
      allow_ifd: false,
      ..EvaluatorConfig::default()
    };
    let policy = NixEvalPolicy::from(&config);
    let opts = policy.nix_options();
    assert!(
      opts
        .iter()
        .any(|(k, v)| k == "restrict-eval" && v == "true")
    );
    assert!(
      opts
        .iter()
        .any(|(k, v)| k == "allow-import-from-derivation" && v == "false")
    );
  }
}
