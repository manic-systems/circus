//! Read `requiredSystemFeatures` out of `nix derivation show` output.

use std::collections::BTreeSet;

use crate::{Error, Result};

fn features_of_structured(sa: &serde_json::Value, out: &mut BTreeSet<String>) {
  if let Some(arr) = sa.get("requiredSystemFeatures").and_then(|v| v.as_array())
  {
    out.extend(
      arr
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .filter(|s| !s.is_empty()),
    );
  }
}

/// Plain drvs flatten the list to a space-separated string in `env`, whereas
/// structuredAttrs drvs carry a real list under `structuredAttrs`.
fn features_of_drv(drv: &serde_json::Value, out: &mut BTreeSet<String>) {
  let Some(drv) = drv.as_object() else {
    return;
  };

  if let Some(sa) = drv.get("structuredAttrs") {
    features_of_structured(sa, out);
    return;
  }

  let Some(env) = drv.get("env").and_then(|v| v.as_object()) else {
    return;
  };
  if let Some(json_str) = env.get("__json").and_then(|v| v.as_str()) {
    if let Ok(sa) = serde_json::from_str::<serde_json::Value>(json_str) {
      features_of_structured(&sa, out);
    }
    return;
  }
  if let Some(env_str) =
    env.get("requiredSystemFeatures").and_then(|v| v.as_str())
  {
    out.extend(
      env_str
        .split(|c: char| c.is_whitespace() || c == ':' || c == ',')
        .filter(|s| !s.is_empty())
        .map(str::to_owned),
    );
  }
}

/// Union of `requiredSystemFeatures` over every derivation in a `nix derivation
/// show` document.
#[must_use]
pub fn union_required_features(parsed: &serde_json::Value) -> Vec<String> {
  let Some(obj) = parsed.as_object() else {
    return Vec::new();
  };
  let drvs = obj
    .get("derivations")
    .and_then(|v| v.as_object())
    .unwrap_or(obj);

  let mut out = BTreeSet::new();
  for drv in drvs.values() {
    features_of_drv(drv, &mut out);
  }
  out.into_iter().collect::<Vec<String>>()
}

/// Run `nix derivation show` over `drvs` and union their
/// `requiredSystemFeatures`.
///
/// # Errors
///
/// Returns [`Error::Eval`] when nix cannot be spawned, exits
/// non-zero, or emits unparseable JSON.
pub async fn show_required_features(drvs: &[String]) -> Result<Vec<String>> {
  let mut features = BTreeSet::new();
  for chunk in drvs.chunks(1024) {
    let out = tokio::process::Command::new("nix")
      .args([
        "--extra-experimental-features",
        "nix-command",
        "derivation",
        "show",
      ])
      .args(chunk)
      .output()
      .await
      .map_err(|e| {
        Error::Eval(format!("failed to run nix derivation show: {e}"))
      })?;
    if !out.status.success() {
      return Err(Error::Eval(format!(
        "nix derivation show failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
      )));
    }
    let parsed = serde_json::from_slice::<serde_json::Value>(&out.stdout)
      .map_err(|e| Error::Eval(format!("nix derivation show output: {e}")))?;
    features.extend(union_required_features(&parsed));
  }
  Ok(features.into_iter().collect())
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;

  #[test]
  fn reads_wrapped_derivation_show() {
    let shown = json!({
      "derivations": {
        "/nix/store/example.drv": {
          "env": {
            "requiredSystemFeatures": "kvm nixos-test uid-range"
          }
        }
      },
      "version": 3
    });

    assert_eq!(union_required_features(&shown), vec![
      "kvm",
      "nixos-test",
      "uid-range"
    ]);
  }

  #[test]
  fn splits_legacy_colon_and_comma_separators() {
    let shown = json!({
      "/nix/store/example.drv": {
        "env": {
          "requiredSystemFeatures": "kvm:nixos-test,uid-range"
        }
      }
    });

    assert_eq!(union_required_features(&shown), vec![
      "kvm",
      "nixos-test",
      "uid-range"
    ]);
  }

  #[test]
  fn unions_structured_attrs_and_env_drvs() {
    let shown = json!({
      "/nix/store/a.drv": {
        "structuredAttrs": { "requiredSystemFeatures": ["kvm", "nixos-test"] }
      },
      "/nix/store/b.drv": {
        "env": { "requiredSystemFeatures": "uid-range kvm" }
      },
      "/nix/store/c.drv": {
        "env": { "__json": "{\"requiredSystemFeatures\":[\"benchmark\"]}" }
      },
      "/nix/store/d.drv": {}
    });

    assert_eq!(union_required_features(&shown), vec![
      "benchmark",
      "kvm",
      "nixos-test",
      "uid-range"
    ]);
  }
}
