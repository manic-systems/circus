use std::time::Duration;

use circus_common::{CiError, error::Result};
use circus_config::EvaluatorConfig;
use futures::StreamExt as _;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use super::{EvalResult, nix_job_from_derivation};

/// Nix evaluation settings derived from the evaluator configuration, forwarded
/// to evix as `(key, value)` options.
pub(super) struct NixEvalPolicy {
  restrict_eval: bool,
  allow_ifd:     bool,
  allowed_uris:  Vec<String>,
}

impl NixEvalPolicy {
  /// Merge extra `allowed-uris`, sorted and deduped.
  pub(super) fn with_extra_allowed_uris(mut self, extra: Vec<String>) -> Self {
    self.allowed_uris.extend(extra);
    self.allowed_uris.sort();
    self.allowed_uris.dedup();
    self
  }

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
  cancel: &CancellationToken,
) -> Result<EvalResult> {
  tracing::info!(
    evaluation = description,
    nix_options = ?config.nix_options,
    "Starting evix evaluation with Nix options"
  );

  let session = evix::Session::open(config)
    .await
    .map_err(|e| evix_eval_failure(description, &format!("{e:#}")))?;

  let collect = async {
    let mut events = session.stream();
    let mut jobs = Vec::new();
    let mut error_count = 0usize;

    while let Some(event) = events.next().await {
      match event
        .map_err(|e| evix_eval_failure(description, &format!("{e:#}")))?
      {
        evix::Event::Derivation(drv) => {
          jobs.push(nix_job_from_derivation(&drv));
        },
        evix::Event::Error(err) => {
          tracing::warn!(job = %err.attr, "evix reported error: {}", err.error);
          error_count += 1;
        },
        evix::Event::AttrSet { .. } => {},
      }
    }
    Ok::<EvalResult, CiError>(EvalResult { jobs, error_count })
  };

  tokio::select! {
    result = tokio::time::timeout(timeout, collect) => match result {
    Ok(result) => {
      let result = result?;
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
      session.cancel();
      Err(CiError::Timeout(format!(
        "Nix evaluation timed out after {timeout:?}"
      )))
    },
    },
    () = cancel.cancelled() => {
      session.cancel();
      Err(CiError::NixEval("Nix evaluation was cancelled".to_string()))
    },
  }
}

fn evix_eval_failure(description: &str, details: &str) -> CiError {
  CiError::NixEval(format!("{description} evix evaluation failed: {details}"))
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
  fn evix_worker_shutdown_error_keeps_worker_context() {
    let err = evix_eval_failure(
      "flake",
      concat!(
        "evix worker closed stdout while reading event for ",
        "packages.x86_64-linux.bad; worker status: exit status: 1; ",
        "worker stderr:\nevix worker failed: locking flake"
      ),
    );

    assert!(
      matches!(&err, CiError::NixEval(_)),
      "expected NixEval error, got {err:?}"
    );
    let CiError::NixEval(message) = err else {
      return;
    };

    assert!(message.contains("flake evix evaluation failed"));
    assert!(message.contains("reading event"));
    assert!(message.contains("packages.x86_64-linux.bad"));
    assert!(message.contains("exit status: 1"));
    assert!(message.contains("locking flake"));
    assert!(!message.contains("worker closed stdout unexpectedly"));
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
  fn with_extra_allowed_uris_merges_sorts_and_dedups() {
    let p = NixEvalPolicy {
      restrict_eval: true,
      allow_ifd:     false,
      allowed_uris:  vec!["https://github.com".to_string()],
    }
    .with_extra_allowed_uris(vec![
      "github:NixOS/nixpkgs".to_string(),
      "https://github.com".to_string(),
    ]);
    let opts = p.nix_options();
    let (_, v) = opts
      .iter()
      .find(|(k, _)| k == "allowed-uris")
      .expect("allowed-uris option must be present");
    assert_eq!(v, "github:NixOS/nixpkgs https://github.com");
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
      allowed_uris: vec![
        "https://releases.nixos.org".to_string(),
        "https://github.com".to_string(),
      ],
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
    assert!(opts.iter().any(|(k, v)| {
      k == "allowed-uris"
        && v == "https://releases.nixos.org https://github.com"
    }));
  }

  #[test]
  fn evix_config_receives_allowed_uris_policy() {
    let config = EvaluatorConfig {
      restrict_eval: true,
      allow_ifd: false,
      allowed_uris: vec![
        "https://releases.nixos.org".to_string(),
        "https://github.com".to_string(),
      ],
      ..EvaluatorConfig::default()
    };

    let evix_config = evix::Config {
      input: evix::Input::Expr("{}".to_string()),
      auto_args: Vec::new(),
      force_recurse: true,
      gc_roots_dir: None,
      workers: 1,
      max_memory_size: 512,
      meta: false,
      show_input_drvs: false,
      override_inputs: Vec::new(),
      nix_options: NixEvalPolicy::from(&config).nix_options(),
      ..evix::Config::default()
    };

    assert_eq!(evix_config.nix_options, vec![
      ("restrict-eval".to_string(), "true".to_string()),
      (
        "allow-import-from-derivation".to_string(),
        "false".to_string()
      ),
      (
        "allowed-uris".to_string(),
        "https://releases.nixos.org https://github.com".to_string()
      ),
    ]);
  }
}
