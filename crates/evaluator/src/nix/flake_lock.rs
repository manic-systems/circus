//! Parse committed lockfiles into Nix `allowed-uris` prefixes.

use serde_json::{Map, Value};

/// A supported committed lockfile.
#[derive(Debug)]
pub enum Lockfile {
  Flake {
    root:  Option<String>,
    nodes: Map<String, Value>,
  },
  Pins(Map<String, Value>),
  Unknown,
}

impl Lockfile {
  /// Parse and identify a lockfile by its JSON structure.
  pub fn parse(contents: &str) -> Result<Self, serde_json::Error> {
    let root: Value = serde_json::from_str(contents)?;
    let Some(entries) = root.as_object() else {
      return Ok(Self::Unknown);
    };

    if let Some(nodes) = entries.get("nodes").and_then(Value::as_object) {
      return Ok(Self::Flake {
        root:  entries
          .get("root")
          .and_then(Value::as_str)
          .map(str::to_owned),
        nodes: nodes.clone(),
      });
    }

    if entries.values().any(|entry| entry.get("type").is_some()) {
      return Ok(Self::Pins(entries.clone()));
    }

    Ok(Self::Unknown)
  }

  /// Return the sorted, deduplicated URI prefixes required by this lockfile.
  #[must_use]
  pub fn allowed_uris(&self) -> Vec<String> {
    match self {
      Self::Flake { root, nodes } => {
        allowed_uris_from_nodes(nodes, root.as_deref())
      },
      Self::Pins(pins) => allowed_uris_from_nodes(pins, None),
      Self::Unknown => Vec::new(),
    }
  }
}

fn allowed_uris_from_nodes(
  nodes: &serde_json::Map<String, Value>,
  root_name: Option<&str>,
) -> Vec<String> {
  let mut uris: Vec<String> = nodes
    .iter()
    .filter(|(name, _)| Some(name.as_str()) != root_name)
    .filter_map(|(_, node)| node.get("locked").or(Some(node)))
    .flat_map(locked_node_to_uris)
    .collect();

  uris.sort();
  uris.dedup();
  uris
}

/// Map one `locked` node to the narrowest prefix(es) `checkURI` accepts.
fn locked_node_to_uris(locked: &Value) -> Vec<String> {
  let typ = locked
    .get("type")
    .and_then(Value::as_str)
    .unwrap_or_default();

  match typ {
    "github" | "gitlab" | "sourcehut" => {
      let (Some(owner), Some(repo)) = (
        locked.get("owner").and_then(Value::as_str),
        locked.get("repo").and_then(Value::as_str),
      ) else {
        return Vec::new();
      };
      // owner/repo, not the full /rev, whose trailing `?narHash` fails checkURI
      vec![format!("{typ}:{owner}/{repo}")]
    },

    "tarball" | "file" => {
      locked
        .get("url")
        .and_then(Value::as_str)
        .map(|url| vec![url.to_owned()])
        .unwrap_or_default()
    },

    "git" | "mercurial" => {
      let scheme = if typ == "git" { "git+" } else { "hg+" };
      locked
        .get("url")
        .and_then(Value::as_str)
        .map(|url| with_scheme_and_parent(scheme, url))
        .unwrap_or_default()
    },

    // `path` inputs are local. `indirect` is resolved to a concrete node.
    "path" | "indirect" => Vec::new(),

    other => {
      let uris = locked
        .get("url")
        .and_then(Value::as_str)
        .map(|url| with_scheme_and_parent("", url))
        .unwrap_or_default();
      tracing::warn!(
        node_type = other,
        derived = ?uris,
        "Unrecognized flake.lock input type, deriving best-effort allowed-uris"
      );
      uris
    },
  }
}

/// Scheme-prefixed url plus its parent dir, covering the `?ref=&rev=` form.
fn with_scheme_and_parent(scheme: &str, url: &str) -> Vec<String> {
  let full = format!("{scheme}{url}");
  let mut out = vec![full.clone()];
  if let Some(slash) = full.rfind('/') {
    out.push(full[..=slash].to_owned());
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn maps_each_input_type_and_skips_local_and_root() {
    let lock = r#"{
      "root": "root", "version": 7,
      "nodes": {
        "root": { "inputs": { "forge": "forge", "chan": "chan", "vcs": "vcs", "local": "local" } },
        "forge": { "locked": { "type": "github", "owner": "ipetkov", "repo": "crane", "rev": "abc" } },
        "chan": { "locked": { "type": "tarball", "url": "https://releases.nixos.org/x/nixexprs.tar.xz" } },
        "vcs": { "locked": { "type": "git", "url": "https://git.example.com/team/lib", "rev": "def" } },
        "local": { "locked": { "type": "path", "path": "/etc/nixos" } }
      }
    }"#;
    assert_eq!(Lockfile::parse(lock).unwrap().allowed_uris(), vec![
      "git+https://git.example.com/team/",
      "git+https://git.example.com/team/lib",
      "github:ipetkov/crane",
      "https://releases.nixos.org/x/nixexprs.tar.xz",
    ]);
  }

  #[test]
  fn dedups_repeated_prefixes() {
    let lock = r#"{
      "root": "root", "version": 7,
      "nodes": {
        "root": { "inputs": { "a": "a" } },
        "a": { "locked": { "type": "github", "owner": "o", "repo": "a", "rev": "1" } },
        "b": { "locked": { "type": "github", "owner": "o", "repo": "a", "rev": "2" } }
      }
    }"#;
    assert_eq!(Lockfile::parse(lock).unwrap().allowed_uris(), vec![
      "github:o/a"
    ]);
  }

  #[test]
  fn unknown_lock_yields_no_uris() {
    assert!(Lockfile::parse("{}").unwrap().allowed_uris().is_empty());
    assert!(Lockfile::parse("not json").is_err());
  }

  #[test]
  fn maps_tack_pins() {
    let lock = r#"{
      "nixpkgs": { "type": "github", "owner": "nixos", "repo": "nixpkgs", "rev": "ffa" },
      "local": { "type": "path", "path": "." }
    }"#;
    assert_eq!(Lockfile::parse(lock).unwrap().allowed_uris(), vec![
      "github:nixos/nixpkgs"
    ]);
  }
}
