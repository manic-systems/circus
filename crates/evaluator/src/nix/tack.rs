//! Derive `allowed-uris` from Tack pins, including the lock files inside them.
//!
//! Tack's resolver evaluates a flake pin against the pin's own `flake.lock`
//! and, for a pin wired through Tack, its own `.tack/pins.lock.json`. Those
//! nested lock files only exist inside the fetched pin source, so the project's
//! committed locks never name the inputs they fetch.

use std::{
  collections::{HashMap, HashSet},
  path::{Component, Path, PathBuf},
};

use circus_common::{CiError, error::Result};
use circus_config::EvaluatorConfig;
use serde::Deserialize;
use serde_json::{Map, Value};
use tokio::{process::Command, time::Instant};
use tokio_util::sync::CancellationToken;

use super::flake_lock::{Lockfile, allowed_uris_from_nodes};

const PINS_ENV: &str = "CIRCUS_TACK_PINS";

#[derive(Debug, Default, Deserialize)]
struct PinsToml {
  #[serde(default)]
  inputs: HashMap<String, PinDecl>,
}

#[derive(Debug, Deserialize)]
struct PinDecl {
  #[serde(rename = "type")]
  kind:  Option<String>,
  flake: Option<bool>,
  dir:   Option<String>,
}

impl PinDecl {
  fn is_flake(&self) -> bool {
    self
      .kind
      .as_deref()
      .map_or_else(|| self.flake.unwrap_or(true), |kind| kind == "flake")
  }

  fn dir(&self) -> Option<&Path> {
    let dir = Path::new(self.dir.as_deref()?);
    dir
      .components()
      .all(|component| matches!(component, Component::Normal(_)))
      .then_some(dir)
  }
}

/// Collect allowed-uris from the Tack project at `root` and, transitively,
/// from the lock files inside its remote flake pins.
///
/// Failures are logged rather than returned since Tack fetches pins lazily, so
/// a pin that cannot be fetched here may never be needed by the evaluation.
pub(super) async fn allowed_uris(
  root: &Path,
  config: &EvaluatorConfig,
  deadline: Instant,
  cancel: &CancellationToken,
) -> Vec<String> {
  let mut uris = Vec::new();
  let mut fetched = HashSet::new();
  let mut projects = vec![root.to_path_buf()];

  while let Some(project) = projects.pop() {
    let Some((decls, nodes)) = read_pins(&project) else {
      continue;
    };
    uris.extend(allowed_uris_from_nodes(&nodes, None));

    let wanted = nodes
      .into_iter()
      .filter(|(name, node)| {
        decls.inputs.get(name).is_none_or(PinDecl::is_flake)
          && is_remote(node)
          && fetched.insert(node.to_string())
      })
      .collect::<Map<_, _>>();
    if wanted.is_empty() {
      continue;
    }

    let sources = match fetch(wanted, config, deadline, cancel).await {
      Ok(sources) => sources,
      Err(error) => {
        tracing::warn!(project = %project.display(), %error, "Failed to fetch Tack pins, deriving no allowed-uris from their lock files");
        continue;
      },
    };
    for (name, source) in sources {
      let flake_dir = match decls.inputs.get(&name).and_then(PinDecl::dir) {
        Some(dir) => source.join(dir),
        None => source,
      };
      if let Some(Lockfile::Flake { root, nodes }) =
        read_lockfile(&flake_dir.join("flake.lock"))
      {
        uris.extend(allowed_uris_from_nodes(&nodes, root.as_deref()));
      }
      projects.push(flake_dir);
    }
  }

  uris.sort();
  uris.dedup();
  uris
}

/// Whether a lock node names a network source. Local `path` and `file://`
/// sources are left to the restricted evaluation.
fn is_remote(node: &Value) -> bool {
  let url = node.get("url").and_then(Value::as_str).unwrap_or_default();
  let has_scheme = |schemes: &[&str]| {
    schemes
      .iter()
      .any(|scheme| url.starts_with(&format!("{scheme}://")))
  };
  match node.get("type").and_then(Value::as_str) {
    Some("github" | "gitlab" | "sourcehut") => true,
    Some("git") => has_scheme(&["https", "http", "ssh", "git"]),
    Some("tarball") => has_scheme(&["https", "http"]),
    _ => false,
  }
}

fn read_pins(project: &Path) -> Option<(PinsToml, Map<String, Value>)> {
  let tack = project.join(".tack");
  let Lockfile::Pins(nodes) = read_lockfile(&tack.join("pins.lock.json"))?
  else {
    return None;
  };
  let pins_toml = tack.join("pins.toml");
  let decls = match std::fs::read_to_string(&pins_toml) {
    Ok(contents) => {
      toml::from_str(&contents).unwrap_or_else(|error| {
        tracing::warn!(path = %pins_toml.display(), %error, "Failed to parse Tack pins, treating every pin as a flake");
        PinsToml::default()
      })
    },
    Err(error) => {
      if error.kind() != std::io::ErrorKind::NotFound {
        tracing::warn!(path = %pins_toml.display(), %error, "Failed to read Tack pins, treating every pin as a flake");
      }
      PinsToml::default()
    },
  };
  Some((decls, nodes))
}

fn read_lockfile(path: &Path) -> Option<Lockfile> {
  let contents = match std::fs::read_to_string(path) {
    Ok(contents) => contents,
    Err(error) => {
      if error.kind() != std::io::ErrorKind::NotFound {
        tracing::warn!(path = %path.display(), %error, "Failed to read lockfile, deriving no allowed-uris");
      }
      return None;
    },
  };
  Lockfile::parse(&contents)
    .inspect_err(|error| {
      tracing::warn!(path = %path.display(), %error, "Failed to parse lockfile, deriving no allowed-uris");
    })
    .ok()
}

/// Fetch the locked pin sources into the store and return their paths.
async fn fetch(
  nodes: Map<String, Value>,
  config: &EvaluatorConfig,
  deadline: Instant,
  cancel: &CancellationToken,
) -> Result<HashMap<String, PathBuf>> {
  let mut command = Command::new("nix");
  command
    .args([
      "eval",
      "--impure",
      "--json",
      "--expr",
      &format!(
        "builtins.mapAttrs (_: node: (builtins.fetchTree node).outPath) \
         (builtins.fromJSON (builtins.getEnv \"{PINS_ENV}\"))"
      ),
    ])
    .env(PINS_ENV, Value::Object(nodes).to_string())
    .kill_on_drop(true);
  crate::memory::MemoryLimit::from(config)
    .apply_to(&mut command)
    .map_err(|error| {
      CiError::NixEval(format!(
        "Failed to apply evaluator memory limit: {error}"
      ))
    })?;

  let output = tokio::select! {
    output = tokio::time::timeout_at(deadline, command.output()) => output.map_err(|_| {
      CiError::Timeout("Fetching Tack pins exhausted the evaluation timeout".to_owned())
    })??,
    () = cancel.cancelled() => {
      return Err(CiError::NixEval("Nix evaluation was cancelled".to_string()));
    },
  };
  if !output.status.success() {
    return Err(CiError::NixEval(
      String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    ));
  }
  Ok(serde_json::from_slice(&output.stdout)?)
}
