//! Flake probe: auto-discover what a Nix flake repository provides.

use serde::{Deserialize, Serialize};

use crate::{CiError, error::Result};

/// Result of probing a flake repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlakeProbeResult {
  pub is_flake:          bool,
  pub outputs:           Vec<FlakeOutput>,
  pub suggested_jobsets: Vec<SuggestedJobset>,
  pub metadata:          FlakeMetadata,
  pub error:             Option<String>,
}

/// A discovered flake output attribute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlakeOutput {
  pub path:        String,
  pub output_type: String,
  pub systems:     Vec<String>,
}

/// A suggested jobset configuration based on discovered outputs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedJobset {
  pub name:           String,
  pub nix_expression: String,
  pub description:    String,
  pub priority:       u8,
}

/// Metadata extracted from the flake.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlakeMetadata {
  pub description: Option<String>,
  pub url:         Option<String>,
}

/// Maximum output size we'll parse from `nix flake show --json` (10 MB).
const MAX_OUTPUT_SIZE: usize = 10 * 1024 * 1024;

/// Convert a repository URL to a nix flake reference.
///
/// GitHub and GitLab URLs are converted to their native flake ref formats
/// (`github:owner/repo`, `gitlab:owner/repo`). Other HTTPS URLs get a
/// `git+` prefix so nix clones via git rather than trying to unpack an
/// archive. URLs that are already valid flake refs are returned as-is.
fn to_flake_ref(url: &str) -> String {
  let url_trimmed = url.trim().trim_end_matches('/');

  // Already a flake ref (github:, gitlab:, git+, path:, sourcehut:, etc.)
  if url_trimmed.contains(':')
    && !url_trimmed.starts_with("http://")
    && !url_trimmed.starts_with("https://")
  {
    return url_trimmed.to_string();
  }

  // Extract host + path from HTTP(S) URLs
  let without_scheme = url_trimmed
    .strip_prefix("https://")
    .or_else(|| url_trimmed.strip_prefix("http://"))
    .unwrap_or(url_trimmed);
  let without_dotgit = without_scheme.trim_end_matches(".git");

  // github.com/owner/repo -> github:owner/repo
  if let Some(path) = without_dotgit.strip_prefix("github.com/") {
    return format!("github:{path}");
  }

  // gitlab.com/owner/repo -> gitlab:owner/repo
  if let Some(path) = without_dotgit.strip_prefix("gitlab.com/") {
    return format!("gitlab:{path}");
  }

  // Any other HTTPS/HTTP URL: prefix with git+ so nix clones it
  if url_trimmed.starts_with("https://") || url_trimmed.starts_with("http://") {
    return format!("git+{url_trimmed}");
  }

  url_trimmed.to_string()
}

/// Probe a flake repository to discover its outputs and suggest jobsets.
///
/// # Errors
///
/// Returns error if nix flake show command fails or times out.
pub async fn probe_flake(
  repo_url: &str,
  revision: Option<&str>,
) -> Result<FlakeProbeResult> {
  let base_ref = to_flake_ref(repo_url);
  let flake_ref = if let Some(rev) = revision {
    format!("{base_ref}?rev={rev}")
  } else {
    base_ref
  };

  let output = tokio::time::timeout(std::time::Duration::from_mins(1), async {
    tokio::process::Command::new("nix")
      .args([
        "--extra-experimental-features",
        "nix-command flakes",
        "flake",
        "show",
        "--json",
        "--no-write-lock-file",
        &flake_ref,
      ])
      .output()
      .await
  })
  .await
  .map_err(|_| CiError::Timeout("Flake probe timed out after 60s".to_string()))?
  .map_err(|e| {
    CiError::NixEval(format!("Failed to run nix flake show: {e}"))
  })?;

  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    // Check for common non-flake case
    if stderr.contains("does not provide attribute")
      || stderr.contains("has no 'flake.nix'")
    {
      return Ok(FlakeProbeResult {
        is_flake:          false,
        outputs:           Vec::new(),
        suggested_jobsets: Vec::new(),
        metadata:          FlakeMetadata::default(),
        error:             Some(
          "Repository does not contain a flake.nix".to_string(),
        ),
      });
    }
    if stderr.contains("denied")
      || stderr.contains("not accessible")
      || stderr.contains("authentication")
    {
      return Err(CiError::NixEval(
        "Repository not accessible. Check URL and permissions.".to_string(),
      ));
    }
    return Err(CiError::NixEval(format!("nix flake show failed: {stderr}")));
  }

  let stdout = String::from_utf8_lossy(&output.stdout);
  if stdout.len() > MAX_OUTPUT_SIZE {
    // For huge repos like nixpkgs, we still parse but only top-level
    tracing::warn!(
      "Flake show output exceeds {}MB, parsing top-level only",
      MAX_OUTPUT_SIZE / (1024 * 1024)
    );
  }

  let raw: serde_json::Value =
    serde_json::from_str(&stdout[..stdout.len().min(MAX_OUTPUT_SIZE)])
      .map_err(|e| {
        CiError::NixEval(format!("Failed to parse flake show output: {e}"))
      })?;

  let Some(top) = raw.as_object() else {
    return Err(CiError::NixEval(
      "Unexpected flake show output format".to_string(),
    ));
  };

  let mut outputs = Vec::new();
  let mut suggested_jobsets = Vec::new();

  // Known output types and their detection
  let output_types: &[(&str, &str, &str, u8)] = &[
    ("hydraJobs", "derivation", "CI Jobs (hydraJobs)", 10),
    ("checks", "derivation", "Checks", 7),
    ("packages", "derivation", "Packages", 6),
    ("devShells", "derivation", "Development Shells", 3),
    (
      "nixosConfigurations",
      "configuration",
      "NixOS Configurations",
      4,
    ),
    ("nixosModules", "module", "NixOS Modules", 2),
    ("overlays", "overlay", "Overlays", 1),
    (
      "legacyPackages",
      "derivation",
      "Legacy Packages (nixpkgs-style)",
      5,
    ),
  ];

  for &(key, output_type, description, priority) in output_types {
    if let Some(val) = top.get(key) {
      let systems = extract_systems(val);
      outputs.push(FlakeOutput {
        path:        key.to_string(),
        output_type: output_type.to_string(),
        systems:     systems.clone(),
      });

      // Generate suggested jobset
      let nix_expression = match key {
        "hydraJobs" => "hydraJobs".to_string(),
        "checks" => "checks".to_string(),
        "packages" => "packages".to_string(),
        "devShells" => "devShells".to_string(),
        "legacyPackages" => "legacyPackages".to_string(),
        _ => continue, // Don't suggest jobsets for non-buildable outputs
      };

      suggested_jobsets.push(SuggestedJobset {
        name: key.to_string(),
        nix_expression,
        description: description.to_string(),
        priority,
      });
    }
  }

  // Sort jobsets by priority (highest first)
  suggested_jobsets.sort_by_key(|j| std::cmp::Reverse(j.priority));

  // Extract metadata from the flake
  let metadata = FlakeMetadata {
    description: top
      .get("description")
      .and_then(|v| v.as_str())
      .map(std::string::ToString::to_string),
    url:         Some(repo_url.to_string()),
  };

  Ok(FlakeProbeResult {
    is_flake: true,
    outputs,
    suggested_jobsets,
    metadata,
    error: None,
  })
}

/// Extract system names from a flake output value (e.g.,
/// `packages.x86_64-linux`).
pub(crate) fn extract_systems(val: &serde_json::Value) -> Vec<String> {
  let mut systems = Vec::new();
  if let Some(obj) = val.as_object() {
    for key in obj.keys() {
      // System names follow the pattern `arch-os` (e.g., x86_64-linux,
      // aarch64-darwin)
      if key.contains('-') && (key.contains("linux") || key.contains("darwin"))
      {
        systems.push(key.clone());
      }
    }
  }
  systems.sort();
  systems
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "Fine in tests")]
mod tests {
  use serde_json::json;

  use super::*;

  #[test]
  fn test_extract_systems_typical_flake() {
    let val = json!({
        "x86_64-linux": { "hello": {} },
        "aarch64-linux": { "hello": {} },
        "x86_64-darwin": { "hello": {} }
    });
    let systems = extract_systems(&val);
    assert_eq!(systems, vec![
      "aarch64-linux",
      "x86_64-darwin",
      "x86_64-linux"
    ]);
  }

  #[test]
  fn test_extract_systems_empty_object() {
    let val = json!({});
    assert!(extract_systems(&val).is_empty());
  }

  #[test]
  fn test_extract_systems_non_system_keys_ignored() {
    let val = json!({
        "x86_64-linux": {},
        "default": {},
        "lib": {},
        "overlay": {}
    });
    let systems = extract_systems(&val);
    assert_eq!(systems, vec!["x86_64-linux"]);
  }

  #[test]
  fn test_extract_systems_non_object_value() {
    let val = json!("string");
    assert!(extract_systems(&val).is_empty());

    let val = json!(null);
    assert!(extract_systems(&val).is_empty());
  }

  #[test]
  fn test_flake_probe_result_serialization() {
    let result = FlakeProbeResult {
      is_flake:          true,
      outputs:           vec![FlakeOutput {
        path:        "packages".to_string(),
        output_type: "derivation".to_string(),
        systems:     vec!["x86_64-linux".to_string()],
      }],
      suggested_jobsets: vec![SuggestedJobset {
        name:           "packages".to_string(),
        nix_expression: "packages".to_string(),
        description:    "Packages".to_string(),
        priority:       6,
      }],
      metadata:          FlakeMetadata {
        description: Some("A test flake".to_string()),
        url:         Some("https://github.com/test/repo".to_string()),
      },
      error:             None,
    };

    let json = serde_json::to_string(&result).unwrap();
    let parsed: FlakeProbeResult = serde_json::from_str(&json).unwrap();
    assert!(parsed.is_flake);
    assert_eq!(parsed.outputs.len(), 1);
    assert_eq!(parsed.suggested_jobsets.len(), 1);
    assert_eq!(parsed.suggested_jobsets[0].priority, 6);
    assert_eq!(parsed.metadata.description.as_deref(), Some("A test flake"));
  }

  #[test]
  fn test_flake_probe_result_not_a_flake() {
    let result = FlakeProbeResult {
      is_flake:          false,
      outputs:           Vec::new(),
      suggested_jobsets: Vec::new(),
      metadata:          FlakeMetadata::default(),
      error:             Some(
        "Repository does not contain a flake.nix".to_string(),
      ),
    };

    let json = serde_json::to_string(&result).unwrap();
    let parsed: FlakeProbeResult = serde_json::from_str(&json).unwrap();
    assert!(!parsed.is_flake);
    assert!(parsed.error.is_some());
  }

  #[test]
  fn test_to_flake_ref_github_https() {
    assert_eq!(
      to_flake_ref("https://github.com/notashelf/rags"),
      "github:notashelf/rags"
    );
    assert_eq!(
      to_flake_ref("https://github.com/NixOS/nixpkgs"),
      "github:NixOS/nixpkgs"
    );
    assert_eq!(
      to_flake_ref("https://github.com/owner/repo.git"),
      "github:owner/repo"
    );
    assert_eq!(
      to_flake_ref("http://github.com/owner/repo"),
      "github:owner/repo"
    );
    assert_eq!(
      to_flake_ref("https://github.com/owner/repo/"),
      "github:owner/repo"
    );
  }

  #[test]
  fn test_to_flake_ref_gitlab_https() {
    assert_eq!(
      to_flake_ref("https://gitlab.com/owner/repo"),
      "gitlab:owner/repo"
    );
    assert_eq!(
      to_flake_ref("https://gitlab.com/group/subgroup/repo.git"),
      "gitlab:group/subgroup/repo"
    );
  }

  #[test]
  fn test_to_flake_ref_already_flake_ref() {
    assert_eq!(to_flake_ref("github:owner/repo"), "github:owner/repo");
    assert_eq!(to_flake_ref("gitlab:owner/repo"), "gitlab:owner/repo");
    assert_eq!(
      to_flake_ref("git+https://example.com/repo.git"),
      "git+https://example.com/repo.git"
    );
    assert_eq!(
      to_flake_ref("path:/some/local/path"),
      "path:/some/local/path"
    );
    assert_eq!(to_flake_ref("sourcehut:~user/repo"), "sourcehut:~user/repo");
  }

  #[test]
  fn test_to_flake_ref_other_https() {
    assert_eq!(
      to_flake_ref("https://codeberg.org/owner/repo"),
      "git+https://codeberg.org/owner/repo"
    );
    assert_eq!(
      to_flake_ref("https://sr.ht/~user/repo"),
      "git+https://sr.ht/~user/repo"
    );
  }

  #[test]
  fn test_suggested_jobset_ordering() {
    let mut jobsets = [
      SuggestedJobset {
        name:           "packages".to_string(),
        nix_expression: "packages".to_string(),
        description:    "Packages".to_string(),
        priority:       6,
      },
      SuggestedJobset {
        name:           "hydraJobs".to_string(),
        nix_expression: "hydraJobs".to_string(),
        description:    "CI Jobs".to_string(),
        priority:       10,
      },
      SuggestedJobset {
        name:           "checks".to_string(),
        nix_expression: "checks".to_string(),
        description:    "Checks".to_string(),
        priority:       7,
      },
    ];

    jobsets.sort_by_key(|j| std::cmp::Reverse(j.priority));
    assert_eq!(jobsets[0].name, "hydraJobs");
    assert_eq!(jobsets[1].name, "checks");
    assert_eq!(jobsets[2].name, "packages");
  }
}
