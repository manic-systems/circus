//! Integration tests for Nix evaluation via evix.
//!
//! Marked `#[ignore]` by default. Run with:
//!   cargo test -p circus-evaluator -- --ignored
//!
//! Requires `nix` in `PATH` with the `flakes` experimental feature enabled.
#![expect(clippy::unwrap_used, clippy::expect_used, reason = "Fine in tests")]

use std::{fs, path::Path, process::Command, time::Duration};

use circus_config::EvaluatorConfig;
use futures::StreamExt;
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

#[tokio::test]
#[ignore = "requires nix in PATH with flakes enabled"]
async fn evaluator_binary_runs_evix_worker_for_root_flake() {
  let dir = TempDir::new().unwrap();
  fs::write(
    dir.path().join("flake.nix"),
    r#"{
  outputs = { self }: let
    system = builtins.currentSystem;
  in {
    packages.${system}.test = derivation {
      name = "circus-worker-smoke-test";
      inherit system;
      builder = "/bin/sh";
    };
  };
}"#,
  )
  .unwrap();
  let commit = git_stage(dir.path());
  let mut url = url::Url::from_file_path(dir.path()).unwrap();
  url.query_pairs_mut().append_pair("rev", &commit);

  let session = evix::Session::open(evix::Config {
    input: evix::Input::Flake(format!("git+{url}")),
    force_recurse: true,
    worker_exe: Some(env!("CARGO_BIN_EXE_circus-evaluator").into()),
    ..evix::Config::default()
  })
  .await
  .expect("evaluator binary should complete the Evix worker handshake");
  let events = session.stream().collect::<Vec<_>>().await;

  assert!(
    events
      .iter()
      .any(|event| matches!(event, Ok(evix::Event::Derivation(_)))),
    "root flake evaluation should emit a derivation: {events:?}"
  );
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
    &commit,
    "packages",
    true,
    Duration::from_mins(2),
    &permissive_config(),
    &[],
    &CancellationToken::new(),
  )
  .await
  .expect("evaluation should succeed");

  assert_eq!(result.error_count, 0, "no attribute errors expected");
  assert_eq!(result.jobs.len(), 1, "expected exactly one job");
  assert!(
    result.jobs[0].name.contains(".test"),
    "job attr path should contain .test, got: {}",
    result.jobs[0].name
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
    &commit,
    "packages",
    true,
    Duration::from_mins(2),
    &permissive_config(),
    &[],
    &CancellationToken::new(),
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
    &commit,
    "packages",
    true,
    Duration::from_secs(30),
    &permissive_config(),
    &[],
    &CancellationToken::new(),
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
async fn eval_timeout_returns_cierror_timeout() {
  let dir = TempDir::new().unwrap();
  // A flake that loops forever to trigger the timeout path.
  fs::write(
    dir.path().join("flake.nix"),
    r"{
  outputs = { self }: let
    system = builtins.currentSystem;
    loop = x: loop x;
  in {
    packages.${system}.hang = loop null;
  };
}",
  )
  .unwrap();
  let commit = git_stage(dir.path());

  let result = circus_evaluator::nix::evaluate(
    dir.path(),
    &commit,
    "packages",
    true,
    Duration::from_millis(500),
    &permissive_config(),
    &[],
    &CancellationToken::new(),
  )
  .await;

  assert!(result.is_err(), "infinite loop should time out");
  let err = result.unwrap_err();
  assert!(
    matches!(err, circus_common::CiError::Timeout(_)),
    "error should be CiError::Timeout, got: {err:?}"
  );
}
