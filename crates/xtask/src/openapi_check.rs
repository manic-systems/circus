//! `OpenAPI` drift detection.
//!
//! The hand-written `OpenAPI` document at
//! `crates/server/src/routes/openapi.rs` is the source of truth for our
//! published REST surface. It is easy for the document to drift when a
//! handler is added or renamed without updating the JSON. This check
//! parses route registrations from the source tree, normalizes them to
//! `OpenAPI`-style paths, and compares against the documented set.
//!
//! Modules under `crates/server/src/routes/` are classified into:
//!
//! - **api**: nested under `/api/v1` in `routes/mod.rs`; must appear in the
//!   `OpenAPI` document.
//! - **public**: lives at the root, exposed to `OpenAPI` by policy (LDAP login,
//!   channel manifests).
//! - **excluded**: intentionally not in `OpenAPI` (cache speaks the Nix binary
//!   cache protocol, dashboard is HTML, etc.).
//!
//! When the check fails, the report lists exactly which routes are missing
//! from the document and which `OpenAPI` entries no longer match any route.

use std::{
  collections::BTreeSet,
  fmt::Write,
  fs,
  path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;

/// Modules whose `.route("/...")` calls are mounted under `/api/v1`.
/// Keep in sync with the `.merge(...)` block inside `routes::router`'s
/// `.nest("/api/v1", ...)`.
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

/// Public modules whose routes also belong in the `OpenAPI` document.
const PUBLIC_DOCUMENTED_MODULES: &[&str] = &["channel_manifests", "ldap"];

/// Modules whose routes are intentionally NOT in the `OpenAPI` document.
/// Kept for documentation and policy review.
#[allow(dead_code)]
const EXCLUDED_MODULES: &[&str] = &[
  "badges",
  "cache",
  "dashboard",
  "health",
  "metrics",
  "oauth",
  "openapi",
  "webhooks",
];

pub fn run() -> Result<()> {
  #![expect(clippy::print_stdout, reason = "xtask CLI output is intentional")]
  let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("../..")
    .canonicalize()?;
  let routes_dir = manifest_dir.join("crates/server/src/routes");
  let openapi_src = routes_dir.join("openapi.rs");

  let documented =
    parse_openapi_paths(&openapi_src).context("parsing openapi.rs")?;

  let mut registered: BTreeSet<String> = BTreeSet::new();
  for module in API_MODULES.iter().chain(PUBLIC_DOCUMENTED_MODULES) {
    let path = routes_dir.join(format!("{module}.rs"));
    let routes = parse_routes_in_file(&path)
      .with_context(|| format!("scanning routes in {}", path.display()))?;
    registered.extend(routes);
  }

  let missing_in_openapi: Vec<_> =
    registered.difference(&documented).cloned().collect();
  let stale_openapi: Vec<_> =
    documented.difference(&registered).cloned().collect();

  // EXCLUDED_MODULES is informational: it documents what we know is in the
  // server but intentionally absent from OpenAPI. We don't enforce that
  // every excluded route lives in one of these modules, but listing them
  // here makes the policy reviewable.
  if !missing_in_openapi.is_empty() || !stale_openapi.is_empty() {
    let mut msg = String::from("OpenAPI drift detected.\n");
    if !missing_in_openapi.is_empty() {
      msg.push_str("\nRoutes registered but not documented in openapi.rs:\n");
      for r in &missing_in_openapi {
        let _ = writeln!(msg, "  - {r}");
      }
    }
    if !stale_openapi.is_empty() {
      msg.push_str(
        "\nOpenAPI paths that no longer match any registered route:\n",
      );
      for r in &stale_openapi {
        let _ = writeln!(msg, "  - {r}");
      }
    }
    msg.push_str(
      "\nFix by updating crates/server/src/routes/openapi.rs OR updating the \
       route module / handler.\n",
    );
    bail!("{msg}");
  }

  println!(
    "OpenAPI drift check passed: {} routes documented across {} modules.",
    registered.len(),
    API_MODULES.len() + PUBLIC_DOCUMENTED_MODULES.len()
  );
  Ok(())
}

/// Parse `.route("/path", ...)` calls from a route module and return the set
/// of fully-qualified, OpenAPI-style paths.
///
/// API modules are prefixed with `/api/v1`; public modules are not.
pub fn parse_routes_in_file(path: &Path) -> Result<BTreeSet<String>> {
  let module = path
    .file_stem()
    .and_then(|s| s.to_str())
    .ok_or_else(|| anyhow!("invalid module path: {}", path.display()))?
    .to_string();

  let body = fs::read_to_string(path)
    .with_context(|| format!("reading {}", path.display()))?;

  // Match `.route("..."` or `.route(\n        "..."`.
  #[expect(
    clippy::expect_used,
    reason = "static regex initializer - invalid regex would be a programming \
              error"
  )]
  let route_re = Regex::new(r#"\.route\(\s*"([^"]+)""#).expect("valid regex");

  let api = API_MODULES.contains(&module.as_str());
  let public = PUBLIC_DOCUMENTED_MODULES.contains(&module.as_str());

  if !api && !public {
    // Excluded modules: return raw paths without prefix (caller may discard).
    let mut out = BTreeSet::new();
    for cap in route_re.captures_iter(&body) {
      out.insert(normalize_path(&cap[1]));
    }
    return Ok(out);
  }

  let prefix = if api { "/api/v1" } else { "" };
  let mut out = BTreeSet::new();
  for cap in route_re.captures_iter(&body) {
    let raw = &cap[1];
    let normalized = normalize_path(raw);
    out.insert(format!("{prefix}{normalized}"));
  }
  Ok(out)
}

/// Axum 0.8 paths use `{name}` style placeholders, which matches `OpenAPI`
/// style already, so no conversion is needed. We do trim trailing slashes
/// (except for the root) to canonicalize.
fn normalize_path(p: &str) -> String {
  if p.len() > 1 && p.ends_with('/') {
    p.trim_end_matches('/').to_string()
  } else {
    p.to_string()
  }
}

/// Extract every top-level `"/path": { ... }` key from the `OpenAPI` document
/// source by parsing the file's `json!(...)` literal naively. We don't
/// actually run the Rust code; we extract the JSON-ish source between the
/// `"paths": {` brace and its matching close, then scan keys.
pub fn parse_openapi_paths(path: &Path) -> Result<BTreeSet<String>> {
  let body = fs::read_to_string(path)
    .with_context(|| format!("reading {}", path.display()))?;

  // Find the `"paths": {` marker, then walk braces to its match.
  let marker = "\"paths\":";
  let start = body
    .find(marker)
    .ok_or_else(|| anyhow!("openapi.rs missing \"paths\" object"))?;
  let after_marker = &body[start + marker.len()..];

  let open_idx = after_marker
    .find('{')
    .ok_or_else(|| anyhow!("openapi.rs malformed: no `{{` after `paths:`"))?;
  let mut depth = 0i32;
  let mut end = None;
  for (i, b) in after_marker[open_idx..].bytes().enumerate() {
    match b {
      b'{' => depth += 1,
      b'}' => {
        depth -= 1;
        if depth == 0 {
          end = Some(open_idx + i);
          break;
        }
      },
      _ => {},
    }
  }
  let end = end.ok_or_else(|| {
    anyhow!("openapi.rs malformed: unbalanced braces in `paths` object")
  })?;

  let paths_block = &after_marker[open_idx + 1..end];

  // Only match string keys that begin with `/` AND sit at the top level of
  // this block. We track depth so we don't pull in nested keys like
  // `"/builds": { "get": { ... "parameters": [ ... "name": "id" ...` etc.
  let mut depth = 0i32;
  let mut bracket_depth = 0i32;
  let mut out = BTreeSet::new();
  let bytes = paths_block.as_bytes();
  let mut i = 0;
  while i < bytes.len() {
    let b = bytes[i];
    match b {
      b'{' => depth += 1,
      b'}' => depth -= 1,
      b'[' => bracket_depth += 1,
      b']' => bracket_depth -= 1,
      b'"' if depth == 0 && bracket_depth == 0 => {
        // Read string literal until unescaped quote.
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
        let key = &paths_block[key_start..j];
        if key.starts_with('/') {
          // Document paths in openapi.rs are written relative to the
          // server URL `/api/v1`, so we re-prefix them to match registered
          // route paths. Exception: explicitly-documented public paths
          // (currently only LDAP login + channel_manifests) are listed
          // with their absolute path; detect by checking for known
          // prefixes.
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

  Ok(out)
}
