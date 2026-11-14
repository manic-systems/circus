//! Flake probe: auto-discover what a Nix flake repository provides.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::{Error, Result, flake, validate};

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
  pub systems:        Vec<String>,
}

/// Metadata extracted from the flake.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FlakeMetadata {
  pub description: Option<String>,
  pub url:         Option<String>,
}

/// Maximum output size we'll parse from a Nix probe response (10 MB).
const MAX_OUTPUT_SIZE: usize = 10 * 1024 * 1024;
const FLAKE_PROBE_TIMEOUT: Duration = Duration::from_mins(1);

const OUTPUT_TYPES: &[(&str, &str, &str, u8)] = &[
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

/// Probe a flake repository to discover its outputs and suggest jobsets.
///
/// # Errors
///
/// Returns error if Nix cannot inspect the flake or the command times out.
pub async fn probe_flake(
  repo_url: &str,
  revision: Option<&str>,
) -> Result<FlakeProbeResult> {
  let parsed_ref = flake::Ref::from_url(repo_url).map_err(Error::Validation)?;
  if let Some(rev) = revision {
    validate::validate_commit_hash(rev).map_err(Error::Validation)?;
  }
  let full_ref = revision.map_or_else(
    || parsed_ref.to_string(),
    |rev| parsed_ref.with_revision(rev),
  );

  tokio::time::timeout(
    FLAKE_PROBE_TIMEOUT,
    probe_flake_outputs(repo_url, &full_ref),
  )
  .await
  .map_err(|_| {
    Error::Timeout(format!(
      "Flake probe timed out after {FLAKE_PROBE_TIMEOUT:?}"
    ))
  })?
}

async fn probe_flake_outputs(
  repo_url: &str,
  full_ref: &str,
) -> Result<FlakeProbeResult> {
  let metadata_output = tokio::process::Command::new("nix")
    .args([
      "--extra-experimental-features",
      "nix-command flakes",
      "flake",
      "metadata",
      "--json",
      "--no-write-lock-file",
      full_ref,
    ])
    .kill_on_drop(true)
    .output()
    .await
    .map_err(|e| {
      Error::Eval(format!("Failed to run nix flake metadata: {e}"))
    })?;

  if !metadata_output.status.success() {
    let stderr = String::from_utf8_lossy(&metadata_output.stderr);
    if is_missing_flake(&stderr) {
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
    return Err(probe_command_error(
      "nix flake metadata",
      &metadata_output.stderr,
    ));
  }

  if metadata_output.stdout.len() > MAX_OUTPUT_SIZE {
    return Err(Error::Eval(format!(
      "Nix flake metadata exceeds {} MB",
      MAX_OUTPUT_SIZE / (1024 * 1024)
    )));
  }

  let metadata: serde_json::Value =
    serde_json::from_slice(&metadata_output.stdout).map_err(|e| {
      Error::Eval(format!("Failed to parse flake metadata output: {e}"))
    })?;

  let mut outputs = Vec::new();
  let mut suggested_jobsets = Vec::new();

  for &(key, output_type, description, priority) in OUTPUT_TYPES {
    let Some(names) = eval_output_names(full_ref, key).await? else {
      continue;
    };
    let systems = extract_system_names(names);

    outputs.push(FlakeOutput {
      path:        key.to_string(),
      output_type: output_type.to_string(),
      systems:     systems.clone(),
    });

    let nix_expression = match key {
      "hydraJobs" => "hydraJobs",
      "checks" => "checks",
      "packages" => "packages",
      "devShells" => "devShells",
      "legacyPackages" => "legacyPackages",
      _ => continue,
    };

    suggested_jobsets.push(SuggestedJobset {
      name: key.to_string(),
      nix_expression: nix_expression.to_string(),
      description: description.to_string(),
      priority,
      systems,
    });
  }

  suggested_jobsets.sort_by_key(|j| std::cmp::Reverse(j.priority));

  Ok(FlakeProbeResult {
    is_flake: true,
    outputs,
    suggested_jobsets,
    metadata: FlakeMetadata {
      description: metadata
        .get("description")
        .and_then(|value| value.as_str())
        .map(std::string::ToString::to_string),
      url:         Some(repo_url.to_string()),
    },
    error: None,
  })
}

async fn eval_output_names(
  full_ref: &str,
  output_name: &str,
) -> Result<Option<Vec<String>>> {
  let output_ref = format!("{full_ref}#{output_name}");
  let output = tokio::process::Command::new("nix")
    .args([
      "--extra-experimental-features",
      "nix-command flakes",
      "eval",
      "--json",
      "--no-write-lock-file",
      &output_ref,
      "--apply",
      "builtins.attrNames",
    ])
    .kill_on_drop(true)
    .output()
    .await
    .map_err(|e| {
      Error::Eval(format!("Failed to inspect flake output {output_name}: {e}"))
    })?;

  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    if is_missing_output(&stderr) {
      return Ok(None);
    }
    return Err(probe_command_error(
      &format!("nix eval {output_name}"),
      &output.stderr,
    ));
  }

  if output.stdout.len() > MAX_OUTPUT_SIZE {
    return Err(Error::Eval(format!(
      "Nix flake output {output_name} exceeds {} MB",
      MAX_OUTPUT_SIZE / (1024 * 1024)
    )));
  }

  serde_json::from_slice(&output.stdout)
    .map(Some)
    .map_err(|e| {
      Error::Eval(format!("Failed to parse flake output {output_name}: {e}"))
    })
}

fn is_missing_output(stderr: &str) -> bool {
  stderr.contains("does not provide attribute")
    || stderr.contains("has no attribute")
    || (stderr.contains("attribute") && stderr.contains("missing"))
}

fn is_missing_flake(stderr: &str) -> bool {
  stderr.contains("has no 'flake.nix'")
    || stderr.contains("does not contain a 'flake.nix'")
}

fn probe_command_error(command: &str, stderr: &[u8]) -> Error {
  let stderr = String::from_utf8_lossy(stderr);
  tracing::warn!(command, stderr = %stderr, "nix flake probe failed");
  let detail = stderr
    .lines()
    .rev()
    .find(|line| !line.trim().is_empty())
    .map_or("unknown Nix error", str::trim);
  Error::Eval(format!("{command} failed: {detail}"))
}

fn extract_system_names(names: Vec<String>) -> Vec<String> {
  let mut systems = names
    .into_iter()
    .filter(|key| {
      key.contains('-') && (key.contains("linux") || key.contains("darwin"))
    })
    .collect::<Vec<_>>();
  systems.sort();
  systems
}

/// Extract system names from a flake output value (e.g.,
/// `packages.x86_64-linux`).
#[cfg(test)]
pub(crate) fn extract_systems(val: &serde_json::Value) -> Vec<String> {
  val.as_object().map_or_else(Vec::new, |top| {
    extract_system_names(top.keys().cloned().collect())
  })
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
    assert_eq!(extract_systems(&val), Vec::<String>::new());
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
    assert_eq!(extract_systems(&val), Vec::<String>::new());

    let val = json!(null);
    assert_eq!(extract_systems(&val), Vec::<String>::new());
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
        systems:        vec!["x86_64-linux".to_string()],
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
        systems:        Vec::new(),
      },
      SuggestedJobset {
        name:           "hydraJobs".to_string(),
        nix_expression: "hydraJobs".to_string(),
        description:    "CI Jobs".to_string(),
        priority:       10,
        systems:        Vec::new(),
      },
      SuggestedJobset {
        name:           "checks".to_string(),
        nix_expression: "checks".to_string(),
        description:    "Checks".to_string(),
        priority:       7,
        systems:        Vec::new(),
      },
    ];

    jobsets.sort_by_key(|j| std::cmp::Reverse(j.priority));
    assert_eq!(jobsets[0].name, "hydraJobs");
    assert_eq!(jobsets[1].name, "checks");
    assert_eq!(jobsets[2].name, "packages");
  }
}
