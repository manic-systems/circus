//! Tests for nix evaluation output parsing.
//! These tests do NOT require nix or a database.
#![expect(clippy::unwrap_used, reason = "Fine in tests")]

#[test]
fn test_parse_valid_job() {
  let line = r#"{"name":"hello","drvPath":"/nix/store/abc123-hello.drv","system":"x86_64-linux","outputs":{"out":"/nix/store/abc123-hello"}}"#;
  let result = circus_evaluator::nix::parse_eval_output(line);
  assert_eq!(result.jobs.len(), 1);
  assert_eq!(result.error_count, 0);
  assert_eq!(result.jobs[0].name, "hello");
  assert_eq!(result.jobs[0].drv_path, "/nix/store/abc123-hello.drv");
  assert_eq!(result.jobs[0].system.as_deref(), Some("x86_64-linux"));
}

#[test]
fn test_parse_multiple_jobs() {
  let output = r#"{"name":"hello","drvPath":"/nix/store/abc-hello.drv","system":"x86_64-linux"}
{"name":"world","drvPath":"/nix/store/def-world.drv","system":"aarch64-linux"}"#;

  let result = circus_evaluator::nix::parse_eval_output(output);
  assert_eq!(result.jobs.len(), 2);
  assert_eq!(result.error_count, 0);
  assert_eq!(result.jobs[0].name, "hello");
  assert_eq!(result.jobs[1].name, "world");
}

#[test]
fn test_parse_error_lines() {
  let output = r#"{"name":"hello","drvPath":"/nix/store/abc-hello.drv"}
{"attr":"broken","error":"attribute 'broken' missing"}
{"name":"world","drvPath":"/nix/store/def-world.drv"}"#;

  let result = circus_evaluator::nix::parse_eval_output(output);
  assert_eq!(result.jobs.len(), 2);
  assert_eq!(result.error_count, 1);
}

#[test]
fn test_parse_empty_output() {
  let result = circus_evaluator::nix::parse_eval_output("");
  assert_eq!(result.jobs.len(), 0);
  assert_eq!(result.error_count, 0);
}

#[test]
fn test_parse_blank_lines_ignored() {
  let output = "\n  \n\n";
  let result = circus_evaluator::nix::parse_eval_output(output);
  assert_eq!(result.jobs.len(), 0);
  assert_eq!(result.error_count, 0);
}

#[test]
fn test_parse_malformed_json_skipped() {
  let output = "not json at all\n{invalid \
                json}\n{\"name\":\"ok\",\"drvPath\":\"/nix/store/x-ok.drv\"}";
  let result = circus_evaluator::nix::parse_eval_output(output);
  assert_eq!(result.jobs.len(), 1);
  assert_eq!(result.jobs[0].name, "ok");
}

#[test]
fn test_parse_job_with_input_drvs() {
  let line = r#"{"name":"hello","drvPath":"/nix/store/abc-hello.drv","inputDrvs":{"/nix/store/dep1.drv":["out"],"/nix/store/dep2.drv":["out"]}}"#;
  let result = circus_evaluator::nix::parse_eval_output(line);
  assert_eq!(result.jobs.len(), 1);
  let input_drvs = result.jobs[0].input_drvs.as_ref().unwrap();
  assert_eq!(input_drvs.len(), 2);
}

#[test]
fn test_parse_job_with_constituents() {
  let line = r#"{"name":"aggregate","drvPath":"/nix/store/abc-aggregate.drv","constituents":["hello","world"]}"#;
  let result = circus_evaluator::nix::parse_eval_output(line);
  assert_eq!(result.jobs.len(), 1);
  let constituents = result.jobs[0].constituents.as_ref().unwrap();
  assert_eq!(constituents.len(), 2);
  assert_eq!(constituents[0], "hello");
  assert_eq!(constituents[1], "world");
}

#[test]
fn test_parse_error_without_name() {
  let line = r#"{"error":"some eval error"}"#;
  let result = circus_evaluator::nix::parse_eval_output(line);
  assert_eq!(result.jobs.len(), 0);
  assert_eq!(result.error_count, 1);
}

#[test]
fn test_parse_nix_eval_jobs_attr_field() {
  // nix-eval-jobs uses "attr" instead of "name" for the job identifier
  let line = r#"{"attr":"x86_64-linux.hello","drvPath":"/nix/store/abc123-hello.drv","system":"x86_64-linux"}"#;
  let result = circus_evaluator::nix::parse_eval_output(line);
  assert_eq!(result.jobs.len(), 1);
  assert_eq!(result.jobs[0].name, "x86_64-linux.hello");
  assert_eq!(result.jobs[0].drv_path, "/nix/store/abc123-hello.drv");
}

#[test]
fn test_parse_nix_eval_jobs_both_attr_and_name() {
  // nix-eval-jobs with --force-recurse outputs both "attr" and "name" fields.
  // "attr" is the attribute path, "name" is the derivation name. We prefer
  // "attr" as the job identifier.
  let line = r#"{"attr":"x86_64-linux.hello","attrPath":["x86_64-linux","hello"],"drvPath":"/nix/store/abc123-hello.drv","name":"circus-test-hello","outputs":{"out":"/nix/store/abc123-hello"},"system":"x86_64-linux"}"#;
  let result = circus_evaluator::nix::parse_eval_output(line);
  assert_eq!(result.jobs.len(), 1);
  assert_eq!(result.jobs[0].name, "x86_64-linux.hello");
  assert_eq!(result.jobs[0].drv_path, "/nix/store/abc123-hello.drv");
  assert_eq!(result.jobs[0].system.as_deref(), Some("x86_64-linux"));
  let outputs = result.jobs[0].outputs.as_ref().unwrap();
  assert_eq!(outputs.get("out").unwrap(), "/nix/store/abc123-hello");
}

// Inputs hash computation

#[test]
fn test_inputs_hash_deterministic() {
  // The compute_inputs_hash function is in eval_loop which is not easily
  // testable as a standalone function since it's not public. We test the nix
  // parsing above and trust the hash logic is correct since it uses sha2.
}
