//! Flake probe: auto-discover what a Nix flake repository provides.

use serde::{Deserialize, Serialize};

use super::flake;
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

/// Probe a flake repository to discover its outputs and suggest jobsets.
///
/// # Errors
///
/// Returns error if nix flake show command fails or times out.
pub async fn probe_flake(
  repo_url: &str,
  revision: Option<&str>,
) -> Result<FlakeProbeResult> {
  let parsed_ref =
    flake::Ref::from_url(repo_url).map_err(CiError::Validation)?;
  if let Some(rev) = revision {
    crate::validate::validate_commit_hash(rev).map_err(CiError::Validation)?;
  }
  let full_ref = revision.map_or_else(
    || parsed_ref.to_string(),
    |rev| parsed_ref.with_revision(rev),
  );

  let output = tokio::time::timeout(std::time::Duration::from_mins(1), async {
    tokio::process::Command::new("nix")
      .args([
        "--extra-experimental-features",
        "nix-command flakes",
        "flake",
        "show",
        "--json",
        "--no-write-lock-file",
        &full_ref,
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
    tracing::warn!(stderr = %stderr, "nix flake probe failed");
    return Err(CiError::NixEval(
      "Repository not accessible or not a flake".to_string(),
    ));
  }

  let stdout = String::from_utf8_lossy(&output.stdout);
  if stdout.len() > MAX_OUTPUT_SIZE {
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

      let nix_expression = match key {
        "hydraJobs" => "hydraJobs".to_string(),
        "checks" => "checks".to_string(),
        "packages" => "packages".to_string(),
        "devShells" => "devShells".to_string(),
        "legacyPackages" => "legacyPackages".to_string(),
        _ => continue,
      };

      suggested_jobsets.push(SuggestedJobset {
        name: key.to_string(),
        nix_expression,
        description: description.to_string(),
        priority,
      });
    }
  }

  suggested_jobsets.sort_by_key(|j| std::cmp::Reverse(j.priority));

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
