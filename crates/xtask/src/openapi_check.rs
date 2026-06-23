//! `OpenAPI` validation and route coverage checks.

use std::{
  collections::BTreeSet,
  fmt::Write,
  fs,
  path::{Path, PathBuf},
};

use color_eyre::{
  Result,
  eyre::{Context, bail},
};
use regex::Regex;
use serde_json::Value;
use utoipa::openapi::OpenApi;

/// Modules whose `.route("/...")` calls are mounted under `/api/v1`.
/// Keep in sync with the `.merge(...)` block inside `routes::router`'s
/// `.nest("/api/v1", ...)`.
pub const API_MODULES: &[&str] = &[
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

/// Public route modules that are also part of the generated API reference.
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

pub fn workspace_root() -> Result<PathBuf> {
  PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    .join("../..")
    .canonicalize()
    .context("finding workspace root")
}

pub fn run() -> Result<()> {
  #![expect(clippy::print_stdout, reason = "xtask CLI output is intentional")]
  let root = workspace_root()?;
  let routes_dir = root.join("crates/server/src/routes");
  let spec = openapi_document();
  let documented = documented_paths(&spec);

  let mut registered = BTreeSet::new();
  for module in API_MODULES {
    let path = routes_dir.join(format!("{module}.rs"));
    registered.extend(parse_routes_in_file(&path, "/api/v1")?);
  }
  for module in PUBLIC_DOCUMENTED_MODULES {
    let path = routes_dir.join(format!("{module}.rs"));
    registered.extend(parse_routes_in_file(&path, "")?);
  }

  let missing_in_openapi: Vec<_> =
    registered.difference(&documented).cloned().collect();
  let stale_openapi: Vec<_> =
    documented.difference(&registered).cloned().collect();

  if !missing_in_openapi.is_empty() || !stale_openapi.is_empty() {
    let mut msg = String::from("OpenAPI drift detected.\n");
    if !missing_in_openapi.is_empty() {
      msg.push_str("\nRoutes registered but not documented in openapi.rs:\n");
      for route in &missing_in_openapi {
        let _ = writeln!(msg, "  - {route}");
      }
    }
    if !stale_openapi.is_empty() {
      msg.push_str(
        "\nOpenAPI paths that no longer match any registered route:\n",
      );
      for route in &stale_openapi {
        let _ = writeln!(msg, "  - {route}");
      }
    }
    msg.push_str(
      "\nFix by updating the utoipa OpenAPI document and the route module \
       together.\n",
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

pub fn openapi_document() -> OpenApi {
  circus_server::routes::openapi::document()
}

fn documented_paths(spec: &OpenApi) -> BTreeSet<String> {
  let value = openapi_value(spec);
  value
    .get("paths")
    .and_then(Value::as_object)
    .into_iter()
    .flat_map(|paths| paths.keys())
    .map(|path| display_path(&value, path))
    .collect()
}

fn parse_routes_in_file(path: &Path, prefix: &str) -> Result<BTreeSet<String>> {
  let body = fs::read_to_string(path)
    .with_context(|| format!("reading {}", path.display()))?;
  let route_re =
    Regex::new(r#"\.route\(\s*"([^"]+)""#).context("compiling route regex")?;
  let mut out = BTreeSet::new();
  for cap in route_re.captures_iter(&body) {
    let normalized = normalize_path(&cap[1]);
    if prefix.is_empty() || normalized.starts_with("/api/") {
      out.insert(normalized);
    } else {
      out.insert(format!("{prefix}{normalized}"));
    }
  }
  Ok(out)
}

pub fn openapi_value(spec: &OpenApi) -> Value {
  serde_json::to_value(spec).unwrap_or_else(|_| serde_json::json!({}))
}

pub fn display_path(spec: &Value, path: &str) -> String {
  let normalized = normalize_path(path);
  if is_absolute_public_path(&normalized) || normalized.starts_with("/api/") {
    normalized
  } else {
    format!("{}{}", api_root(spec), normalized)
  }
}

fn api_root(spec: &Value) -> &str {
  spec
    .get("servers")
    .and_then(Value::as_array)
    .and_then(|servers| servers.first())
    .and_then(|server| server.get("url"))
    .and_then(Value::as_str)
    .map(|url| url.trim_end_matches('/'))
    .filter(|url| !url.is_empty())
    .unwrap_or("/api/v1")
}

fn is_absolute_public_path(path: &str) -> bool {
  path == "/auth/ldap"
    || path == "/health"
    || path == "/prometheus"
    || path.starts_with("/channel/")
    || path.starts_with("/job/")
    || path == "/nix-cache"
    || path.starts_with("/nix-cache/")
    || path == "/projects/{project}/nix-cache"
    || (path.starts_with("/projects/") && path.contains("/nix-cache/"))
}

fn normalize_path(path: &str) -> String {
  if path.len() > 1 && path.ends_with('/') {
    path.trim_end_matches('/').to_string()
  } else {
    path.to_string()
  }
}

pub fn method_rank(method: &str) -> usize {
  match method {
    "GET" => 0,
    "HEAD" => 1,
    "OPTIONS" => 2,
    "POST" => 3,
    "PUT" => 4,
    "PATCH" => 5,
    "DELETE" => 6,
    _ => 99,
  }
}
