//! Integration tests for Nix evaluation via evix.
//!
//! Marked `#[ignore]` by default. Run with:
//!   cargo test -p circus-evaluator -- --ignored
//!
//! Requires `nix` in `PATH` with the `flakes` experimental feature enabled.
#![expect(clippy::unwrap_used, clippy::expect_used, reason = "Fine in tests")]

use std::{fs, path::Path, process::Command, time::Duration};

use circus_config::EvaluatorConfig;
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

fn git_stage(dir: &Path) -> String {
  for args in [
    &["init", "-q"][..],
    &["config", "user.email", "test@circus"],
    &["config", "user.name", "Circus Test"],
    &["config", "commit.gpgsign", "false"],
    &["add", "."],
    &["commit", "-qm", "fixture"],
  ] {
    assert!(
      Command::new("git")
        .args(args)
        .current_dir(dir)
        .status()
        .expect("git command failed")
        .success(),
      "git {args:?} failed"
    );
  }
  String::from_utf8(
    Command::new("git")
      .args(["rev-parse", "HEAD"])
      .current_dir(dir)
      .output()
      .expect("read fixture commit")
      .stdout,
  )
  .expect("fixture commit is UTF-8")
  .trim()
  .to_owned()
}

fn permissive_config() -> EvaluatorConfig {
  EvaluatorConfig {
    restrict_eval: false,
    allow_ifd: true,
    ..EvaluatorConfig::default()
  }
}

fn worker_exe() -> &'static Path {
  Path::new(env!("CARGO_BIN_EXE_circus-evaluator"))
}

#[tokio::test]
#[ignore = "requires nix in PATH with flakes enabled"]
async fn eval_minimal_flake_returns_one_job() {
  let dir = TempDir::new().unwrap();
  fs::write(
    dir.path().join("flake.nix"),
    r#"{
  outputs = { self }: let
    system = builtins.currentSystem;
  in {
    packages.${system}.test = derivation {
      name = "circus-eval-test";
      inherit system;
      builder = "/bin/sh";
    };
  };
}"#,
  )
  .unwrap();
  let commit = git_stage(dir.path());

  let result = circus_evaluator::nix::evaluate(
    dir.path(),
    dir.path().to_str().unwrap(),
    &commit,
    ".",
    true,
    Duration::from_mins(2),
    &permissive_config(),
    &[],
    &CancellationToken::new(),
    Some(worker_exe()),
  )
  .await
  .expect("evaluation should succeed");

  assert_eq!(result.error_count, 0, "no attribute errors expected");
  assert!(
    result.jobs.iter().any(|job| job.name.contains(".test")),
    "root evaluation should contain the test job: {:?}",
    result.jobs
  );
}

#[tokio::test]
#[ignore = "requires nix in PATH with flakes enabled"]
async fn eval_captures_per_attribute_errors_without_failing_fatally() {
  let dir = TempDir::new().unwrap();
  fs::write(
    dir.path().join("flake.nix"),
    r#"{
  outputs = { self }: let
    system = builtins.currentSystem;
  in {
    packages.${system} = {
      good = derivation {
        name = "circus-good";
        inherit system;
        builder = "/bin/sh";
      };
      broken = builtins.throw "intentional test failure";
    };
  };
}"#,
  )
  .unwrap();
  let commit = git_stage(dir.path());

  let result = circus_evaluator::nix::evaluate(
    dir.path(),
    dir.path().to_str().unwrap(),
    &commit,
    "packages",
    true,
    Duration::from_mins(2),
    &permissive_config(),
    &[],
    &CancellationToken::new(),
    Some(worker_exe()),
  )
  .await
  .expect("fatal eval error not expected for per-attribute throws");

  assert_eq!(
    result.jobs.len(),
    1,
    "only the good package should be reported"
  );
  assert!(
    result.error_count > 0,
    "broken attribute must be reported as an error"
  );
}

#[tokio::test]
#[ignore = "requires nix in PATH with flakes enabled"]
async fn eval_fatal_parse_error_returns_cierror_nixeval() {
  let dir = TempDir::new().unwrap();
  fs::write(dir.path().join("flake.nix"), "not valid nix syntax at all")
    .unwrap();
  let commit = git_stage(dir.path());

  let result = circus_evaluator::nix::evaluate(
    dir.path(),
    dir.path().to_str().unwrap(),
    &commit,
    "packages",
    true,
    Duration::from_secs(30),
    &permissive_config(),
    &[],
    &CancellationToken::new(),
    Some(worker_exe()),
  )
  .await;

  assert!(
    result.is_err(),
    "syntax error in flake.nix should propagate as Err"
  );
  let err = result.unwrap_err();
  assert!(
    matches!(err, circus_common::CiError::NixEval(_)),
    "error should be CiError::NixEval, got: {err:?}"
  );
}

#[tokio::test]
#[ignore = "requires nix in PATH with flakes enabled"]
async fn eval_zero_timeout_returns_cierror_timeout() {
  let dir = TempDir::new().unwrap();
  fs::write(
    dir.path().join("flake.nix"),
    r#"{
  outputs = { self }: let
    system = builtins.currentSystem;
  in {
    packages.${system}.test = derivation {
      name = "circus-timeout-test";
      inherit system;
      builder = "/bin/sh";
    };
  };
}"#,
  )
  .unwrap();
  let commit = git_stage(dir.path());

  let result = circus_evaluator::nix::evaluate(
    dir.path(),
    dir.path().to_str().unwrap(),
    &commit,
    "packages",
    true,
    Duration::ZERO,
    &permissive_config(),
    &[],
    &CancellationToken::new(),
    Some(worker_exe()),
  )
  .await;

  assert!(result.is_err(), "a zero timeout must expire");
  let err = result.unwrap_err();
  assert!(
    matches!(err, circus_common::CiError::Timeout(_)),
    "error should be CiError::Timeout, got: {err:?}"
  );
}
