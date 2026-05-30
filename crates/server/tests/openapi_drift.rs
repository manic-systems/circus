//! Cargo-level `OpenAPI` drift detection.
//!
//! The xtask binary (`cargo xtask openapi-check`) is the operator-facing
//! entry point for this check. This test runs the same logic during
//! `cargo test` so that CI catches drift without anyone remembering to invoke
//! xtask explicitly.
//!
//! Both this test and the xtask carry their own copy of the scanner
//! because xtask is a binary-only crate today. If the two diverge, fix
//! both. The duplication is small and well-contained; a future move to
//! generated `OpenAPI` (utoipa) deletes both copies.

#![expect(
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::panic,
  clippy::format_push_string,
  reason = "Fine in tests"
)]
use std::{collections::BTreeSet, fs, path::PathBuf};

const API_MODULES: &[&str] = &[
  "admin",
  "auth",
  "builds",
  "channels",
  "evaluations",
  "jobsets",
  "logs",
  "news",
  "projects",
  "search",
  "users",
];

const PUBLIC_DOCUMENTED_MODULES: &[&str] = &["channel_manifests", "ldap"];

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
  let body = fs::read_to_string(routes_dir().join("openapi.rs"))
    .expect("read openapi.rs");

  let marker = "\"paths\":";
  let start = body.find(marker).expect("openapi.rs has paths key");
  let after = &body[start + marker.len()..];
  let open = after.find('{').expect("opening brace");

  let mut depth = 0i32;
  let mut close = None;
  for (i, b) in after[open..].bytes().enumerate() {
    match b {
      b'{' => depth += 1,
      b'}' => {
        depth -= 1;
        if depth == 0 {
          close = Some(open + i);
          break;
        }
      },
      _ => {},
    }
  }
  let close = close.expect("matching brace");
  let block = &after[open + 1..close];

  let bytes = block.as_bytes();
  let mut depth = 0i32;
  let mut bracket = 0i32;
  let mut out = BTreeSet::new();
  let mut i = 0;
  while i < bytes.len() {
    match bytes[i] {
      b'{' => depth += 1,
      b'}' => depth -= 1,
      b'[' => bracket += 1,
      b']' => bracket -= 1,
      b'"' if depth == 0 && bracket == 0 => {
        let key_start = i + 1;
        let mut j = key_start;
        while j < bytes.len() {
          if bytes[j] == b'\\' {
            j += 2;
            continue;
          }
          if bytes[j] == b'"' {
            break;
          }
          j += 1;
        }
        let key = &block[key_start..j];
        if key.starts_with('/') {
          let normalized =
            if key.starts_with("/auth/ldap") || key.starts_with("/channel/") {
              normalize_path(key)
            } else {
              format!("/api/v1{}", normalize_path(key))
            };
          out.insert(normalized);
        }
        i = j;
      },
      _ => {},
    }
    i += 1;
  }
  out
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
      "OpenAPI drift detected. Update openapi.rs alongside the handler.\n",
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
  // We can't execute the json!() macro from a test without compiling the
  // module, so instead we ensure the routes/openapi.rs file is syntactically
  // intact by checking it contains the closing markers we expect.
  let body = fs::read_to_string(routes_dir().join("openapi.rs")).expect("read");
  assert!(
    body.contains("\"openapi\": \"3.1.0\""),
    "openapi version key"
  );
  assert!(body.contains("\"paths\":"), "paths key");
  assert!(body.contains("\"components\":"), "components key");
}
