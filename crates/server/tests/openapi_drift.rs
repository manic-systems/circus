//! Cargo-level `OpenAPI` drift detection.
//!
//! The xtask binary (`cargo xtask openapi-check`) is the operator-facing
//! entry point for this check. This test runs the same logic during
//! `cargo test` so that CI catches drift without anyone remembering to invoke
//! xtask explicitly.
//!
//! Both this test and the xtask carry their own copy of the route scanner
//! because xtask is a binary-only crate today. If the two diverge, fix both.

#![expect(
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::panic,
  clippy::format_push_string,
  reason = "Fine in tests"
)]
use std::{collections::BTreeSet, fs, path::PathBuf};

use serde_json::Value;

const API_MODULES: &[&str] = &[
  "admin",
  "auth",
  "builds",
  "channels",
  "evaluations",
  "jobsets",
  "logs",
  "news",
  "operator",
  "projects",
  "search",
  "users",
];

const PUBLIC_DOCUMENTED_MODULES: &[&str] = &[
  "badges",
  "cache",
  "channel_manifests",
  "health",
  "ldap",
  "metrics",
  "oauth",
  "openapi",
  "webhooks",
];

fn routes_dir() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/routes")
}

fn normalize_path(p: &str) -> String {
  if p.len() > 1 && p.ends_with('/') {
    p.trim_end_matches('/').to_string()
  } else {
    p.to_string()
  }
}

fn scan_routes(module: &str, prefix: &str) -> BTreeSet<String> {
  let path = routes_dir().join(format!("{module}.rs"));
  let body = fs::read_to_string(&path).unwrap_or_else(|e| {
    panic!("failed to read {}: {e}", path.display());
  });

  let re = regex::Regex::new(r#"\.route\(\s*"([^"]+)""#).unwrap();
  let mut out = BTreeSet::new();
  for cap in re.captures_iter(&body) {
    out.insert(format!("{prefix}{}", normalize_path(&cap[1])));
  }
  out
}

fn parse_documented_paths() -> BTreeSet<String> {
  let spec = circus_server::routes::openapi::document();
  let value = serde_json::to_value(spec).expect("serialize OpenAPI document");
  let api_root = value
    .get("servers")
    .and_then(Value::as_array)
    .and_then(|servers| servers.first())
    .and_then(|server| server.get("url"))
    .and_then(Value::as_str)
    .map(|url| url.trim_end_matches('/'))
    .filter(|url| !url.is_empty())
    .unwrap_or("/api/v1");

  value
    .get("paths")
    .and_then(Value::as_object)
    .into_iter()
    .flat_map(|paths| paths.keys())
    .map(|path| {
      let normalized = normalize_path(path);
      if normalized == "/auth/ldap"
        || normalized == "/health"
        || normalized == "/prometheus"
        || normalized.starts_with("/channel/")
        || normalized.starts_with("/job/")
        || normalized == "/nix-cache"
        || normalized.starts_with("/nix-cache/")
        || normalized == "/projects/{project}/nix-cache"
        || (normalized.starts_with("/projects/")
          && normalized.contains("/nix-cache/"))
        || normalized.starts_with("/api/")
      {
        normalized
      } else {
        format!("{api_root}{normalized}")
      }
    })
    .collect()
}

#[test]
fn openapi_document_covers_every_registered_api_route() {
  let mut registered = BTreeSet::new();
  for m in API_MODULES {
    registered.extend(scan_routes(m, "/api/v1"));
  }
  for m in PUBLIC_DOCUMENTED_MODULES {
    registered.extend(scan_routes(m, ""));
  }

  let documented = parse_documented_paths();

  let missing: Vec<_> = registered.difference(&documented).collect();
  let stale: Vec<_> = documented.difference(&registered).collect();

  if !missing.is_empty() || !stale.is_empty() {
    let mut msg = String::from(
      "OpenAPI drift detected. Update the OpenAPI document alongside the \
       handler.\n",
    );
    if !missing.is_empty() {
      msg.push_str("\nMissing in openapi.rs:\n");
      for r in &missing {
        msg.push_str(&format!("  - {r}\n"));
      }
    }
    if !stale.is_empty() {
      msg.push_str("\nStale openapi.rs entries with no matching route:\n");
      for r in &stale {
        msg.push_str(&format!("  - {r}\n"));
      }
    }
    panic!("{msg}");
  }
}

#[test]
fn openapi_document_parses_as_valid_json() {
  let spec = circus_server::routes::openapi::document();
  let value = serde_json::to_value(spec).expect("serialize OpenAPI document");
  assert_eq!(value["openapi"], "3.1.0");
  assert!(value.get("paths").is_some(), "paths key");
  assert!(value.get("components").is_some(), "components key");
}
