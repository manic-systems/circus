//! Parse committed lockfiles into Nix `allowed-uris` prefixes.

use serde::Deserialize;
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

pub(super) fn allowed_uris_from_nodes(
  nodes: &serde_json::Map<String, Value>,
  root_name: Option<&str>,
) -> Vec<String> {
  let mut uris: Vec<String> = nodes
    .iter()
    .filter(|(name, _)| Some(name.as_str()) != root_name)
    .map(|(_, node)| LockedSource::new(node.get("locked").unwrap_or(node)))
    .flat_map(LockedSource::allowed_uris)
    .collect();

  uris.sort();
  uris.dedup();
  uris
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PathLock {
  path:          String,
  last_modified: Option<u64>,
  nar_hash:      Option<String>,
}

/// A source entry from any supported lockfile.
struct LockedSource<'a> {
  value: &'a Value,
}

impl<'a> LockedSource<'a> {
  const fn new(value: &'a Value) -> Self {
    Self { value }
  }

  fn kind(&self) -> &str {
    self
      .value
      .get("type")
      .and_then(Value::as_str)
      .unwrap_or_default()
  }

  fn url(&self) -> Option<&str> {
    self.value.get("url").and_then(Value::as_str)
  }

  /// Nix matches the full URI including its query, so no prefix works.
  fn store_path_uri(&self) -> Option<String> {
    let lock = PathLock::deserialize(self.value).ok()?;

    if !lock.path.starts_with("/nix/store/") {
      return None;
    }

    let mut query = Vec::new();

    if let Some(last_modified) = lock.last_modified {
      query.push(format!("lastModified={last_modified}"));
    }

    if let Some(nar_hash) = &lock.nar_hash {
      query.push(format!("narHash={}", query_encode(nar_hash)));
    }

    if query.is_empty() {
      Some(format!("path:{}", lock.path))
    } else {
      Some(format!("path:{}?{}", lock.path, query.join("&")))
    }
  }

  fn allowed_uris(self) -> Vec<String> {
    match self.kind() {
      "github" | "gitlab" | "sourcehut" => {
        let (Some(owner), Some(repo)) = (
          self.value.get("owner").and_then(Value::as_str),
          self.value.get("repo").and_then(Value::as_str),
        ) else {
          return Vec::new();
        };
        // owner/repo, not the full /rev, whose trailing `?narHash` fails
        // checkURI.
        vec![format!("{}:{owner}/{repo}", self.kind())]
      },

      "tarball" | "file" => {
        self
          .url()
          .map(UriPrefixes::from_uri)
          .map(UriPrefixes::into_vec)
          .unwrap_or_default()
      },

      "git" | "mercurial" => {
        let scheme = if self.kind() == "git" { "git+" } else { "hg+" };
        self
          .url()
          .map(|url| UriPrefixes::from_uri(format!("{scheme}{url}")))
          .map(UriPrefixes::into_vec)
          .unwrap_or_default()
      },

      "path" => self.store_path_uri().into_iter().collect(),

      // `indirect` is resolved to a concrete node.
      "indirect" => Vec::new(),

      other => {
        let uris = self
          .url()
          .map(UriPrefixes::from_uri)
          .map(UriPrefixes::into_vec)
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
}

/// Percent-encodes like Nix's `encodeQuery`, which keeps RFC 3986 unreserved
/// characters plus `:@/?`.
fn query_encode(value: &str) -> String {
  value
    .bytes()
    .map(|byte| {
      if byte.is_ascii_alphanumeric() || b"-._~:@/?".contains(&byte) {
        char::from(byte).to_string()
      } else {
        format!("%{byte:02X}")
      }
    })
    .collect()
}

/// URI prefixes that satisfy Nix's exact-or-slash-delimited matcher.
pub(super) struct UriPrefixes(Vec<String>);

impl UriPrefixes {
  pub(super) fn from_uri(uri: impl Into<String>) -> Self {
    let uri = uri.into();
    let uri = uri.split(['?', '#']).next().unwrap_or(&uri);
    let mut prefixes = vec![uri.to_owned()];
    // Only a slash inside the path, or a bare `https://` would allow any host.
    let path_start = uri.find("://").map_or(0, |scheme_end| {
      uri[scheme_end + 3..]
        .find('/')
        .map_or(uri.len(), |slash| scheme_end + 3 + slash)
    });
    if let Some(slash) = uri.rfind('/').filter(|&slash| slash >= path_start) {
      prefixes.push(uri[..=slash].to_owned());
    }
    Self(prefixes)
  }

  pub(super) fn into_vec(self) -> Vec<String> {
    self.0
  }
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
    assert_eq!(
      Lockfile::parse(lock)
        .expect("valid lockfile")
        .allowed_uris(),
      vec![
        "git+https://git.example.com/team/",
        "git+https://git.example.com/team/lib",
        "github:ipetkov/crane",
        "https://releases.nixos.org/x/",
        "https://releases.nixos.org/x/nixexprs.tar.xz",
      ]
    );
  }

  #[test]
  fn tarball_parent_allows_nix_fetch_metadata() {
    let lock = r#"{
      "root": "root", "nodes": {
        "root": {},
        "nixpkgs": { "locked": {
          "type": "tarball",
          "url": "https://releases.nixos.org/nixos/unstable/nixos-26.11pre1035164.753cc8a3a874/nixexprs.tar.xz"
        } }
      }
    }"#;
    assert!(Lockfile::parse(lock).expect("valid lockfile").allowed_uris().contains(
      &"https://releases.nixos.org/nixos/unstable/nixos-26.11pre1035164.753cc8a3a874/".to_string()
    ));
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
    assert_eq!(
      Lockfile::parse(lock)
        .expect("valid lockfile")
        .allowed_uris(),
      vec!["github:o/a"]
    );
  }

  #[test]
  fn unknown_lock_yields_no_uris() {
    assert!(
      Lockfile::parse("{}")
        .expect("valid empty JSON object")
        .allowed_uris()
        .is_empty()
    );
    assert!(Lockfile::parse("not json").is_err());
  }

  #[test]
  fn maps_tack_pins() {
    let lock = r#"{
      "nixpkgs": { "type": "github", "owner": "nixos", "repo": "nixpkgs", "rev": "ffa" },
      "local": { "type": "path", "path": "." }
    }"#;
    assert_eq!(
      Lockfile::parse(lock)
        .expect("valid lockfile")
        .allowed_uris(),
      vec!["github:nixos/nixpkgs"]
    );
  }
}
