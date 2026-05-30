//! Host-agnostic smoke tests for the `circus-migrate` binary.
//!
//! These tests invoke the compiled binary and exercise CLI surface that does
//! not require a database: `--help`, `create`, and argument validation.
//! Tests that need a live postgres are covered in the `circus-migrations`
//! integration suite.

#![expect(clippy::unwrap_used, clippy::expect_used, reason = "Fine in tests")]

use std::{path::Path, process::Command};

fn bin() -> &'static Path {
  Path::new(env!("CARGO_BIN_EXE_circus-migrate"))
}

#[test]
fn help_succeeds_and_lists_subcommands() {
  let output = Command::new(bin())
    .arg("--help")
    .output()
    .expect("run circus-migrate --help");

  assert!(output.status.success(), "--help should exit 0");
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(stdout.contains("up"), "help should mention `up`");
  assert!(
    stdout.contains("validate"),
    "help should mention `validate`"
  );
  assert!(stdout.contains("create"), "help should mention `create`");
}

#[test]
fn no_args_exits_nonzero() {
  let output = Command::new(bin())
    .output()
    .expect("run circus-migrate with no args");
  assert!(
    !output.status.success(),
    "no-args invocation should fail and print usage"
  );
}

#[test]
fn create_writes_to_explicit_output_dir() {
  let tmp = tempfile::tempdir().expect("tempdir");
  let output = Command::new(bin())
    .args(["create", "smoke_test", "--output-dir"])
    .arg(tmp.path())
    .output()
    .expect("run circus-migrate create");

  assert!(
    output.status.success(),
    "create failed: stderr={}",
    String::from_utf8_lossy(&output.stderr)
  );

  let entries: Vec<_> = std::fs::read_dir(tmp.path())
    .expect("read tempdir")
    .filter_map(Result::ok)
    .collect();

  assert_eq!(entries.len(), 1, "expected exactly one migration file");

  let path = entries[0].path();
  let name = path.file_name().unwrap().to_string_lossy().into_owned();
  assert!(
    name.ends_with("_smoke_test.sql"),
    "unexpected filename: {name}"
  );

  let body = std::fs::read_to_string(&path).expect("read file");
  assert!(body.contains("-- Migration: smoke_test"));
}

#[test]
fn create_rejects_invalid_name() {
  let tmp = tempfile::tempdir().expect("tempdir");
  let output = Command::new(bin())
    .args(["create", "bad name with spaces", "--output-dir"])
    .arg(tmp.path())
    .output()
    .expect("run circus-migrate create");

  assert!(
    !output.status.success(),
    "create should reject names with spaces"
  );
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert!(
    !stderr.is_empty(),
    "expected an error message on rejected name"
  );
}
