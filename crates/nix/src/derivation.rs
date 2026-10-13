//! Read `requiredSystemFeatures`, outputs, and input derivations out of
//! `nix derivation show` output.

use std::{
  collections::{BTreeSet, HashMap},
  path::{Path, PathBuf},
};

use tokio::{process::Command, sync::OnceCell};

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
    let out = required_features_command(chunk)
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

/// `requiredSystemFeatures` of a single derivation's `nix derivation show`
/// value, as opposed to [`union_required_features`] over a whole document.
#[must_use]
pub fn drv_required_features(drv: &serde_json::Value) -> Vec<String> {
  let mut out = BTreeSet::new();
  features_of_drv(drv, &mut out);
  out.into_iter().collect()
}

/// Store directory of the local Nix. Derivation show format v3 and later print
/// store paths as basenames, so the directory has to come from Nix itself.
///
/// # Errors
///
/// Returns [`Error::Eval`] when nix cannot be run or prints no directory.
pub async fn store_dir() -> Result<&'static Path> {
  static STORE_DIR: OnceCell<PathBuf> = OnceCell::const_new();

  STORE_DIR
    .get_or_try_init(|| {
      async {
        let out = Command::new("nix")
          .args([
            "--extra-experimental-features",
            "nix-command",
            "eval",
            "--raw",
            "--expr",
            "builtins.storeDir",
          ])
          .kill_on_drop(true)
          .output()
          .await
          .map_err(|e| Error::Eval(format!("failed to run nix eval: {e}")))?;

        let dir = String::from_utf8_lossy(&out.stdout).trim().to_owned();
        if !out.status.success() || !dir.starts_with('/') {
          return Err(Error::Eval(format!(
            "could not read the nix store directory: {}",
            String::from_utf8_lossy(&out.stderr).trim()
          )));
        }

        Ok(PathBuf::from(dir))
      }
    })
    .await
    .map(PathBuf::as_path)
}

/// Absolute store path for a path printed by `nix derivation show`, which may
/// be a basename.
#[must_use]
pub fn store_path(store_dir: &Path, path: &str) -> String {
  if path.starts_with('/') {
    path.to_owned()
  } else {
    store_dir.join(path).to_string_lossy().into_owned()
  }
}

/// System a single derivation value builds for.
#[must_use]
pub fn drv_system(drv: &serde_json::Value) -> Option<String> {
  drv
    .get("system")
    .and_then(serde_json::Value::as_str)
    .map(str::to_owned)
}

/// Output paths of a single derivation value, keyed by output name. Floating
/// content-addressed outputs have no path until they are built, so they are
/// left out.
#[must_use]
pub fn drv_outputs(
  store_dir: &Path,
  drv: &serde_json::Value,
) -> Option<HashMap<String, String>> {
  let outputs = drv
    .get("outputs")?
    .as_object()?
    .iter()
    .filter_map(|(name, output)| {
      output
        .get("path")
        .or_else(|| output.get("outPath"))
        .and_then(serde_json::Value::as_str)
        .map(|path| (name.clone(), store_path(store_dir, path)))
    })
    .collect::<HashMap<_, _>>();
  (!outputs.is_empty()).then_some(outputs)
}

/// Input derivations of a single derivation value, keyed by store path, read
/// from `inputDrvs` or `inputs.drvs`.
#[must_use]
pub fn drv_input_drvs(
  store_dir: &Path,
  drv: &serde_json::Value,
) -> Option<HashMap<String, serde_json::Value>> {
  let inputs = drv
    .get("inputDrvs")
    .or_else(|| drv.get("inputs")?.get("drvs"))?
    .as_object()?
    .iter()
    .map(|(path, outputs)| (store_path(store_dir, path), outputs.clone()))
    .collect::<HashMap<_, _>>();
  (!inputs.is_empty()).then_some(inputs)
}

/// Construct the canonical `nix derivation show` command used to inspect
/// required system features.
#[must_use]
pub fn required_features_command(drvs: &[String]) -> Command {
  let mut command = Command::new("nix");
  command
    .args([
      "--extra-experimental-features",
      "nix-command",
      "derivation",
      "show",
    ])
    .args(drvs);
  command
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
  fn reads_features_of_a_single_drv_value() {
    let drv = json!({
      "env": { "requiredSystemFeatures": "kvm nixos-test" }
    });

    assert_eq!(drv_required_features(&drv), vec!["kvm", "nixos-test"]);
    assert!(union_required_features(&drv).is_empty());
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

  #[test]
  fn reads_basename_inputs_and_outputs() {
    let drv = json!({
      "inputs": { "drvs": { "aaa-leaf.drv": ["out"] }, "srcs": [] },
      "outputs": { "out": { "path": "bbb-hidden" } }
    });
    let store_dir = Path::new("/custom/store");

    let inputs = drv_input_drvs(store_dir, &drv).expect("input drvs");
    assert!(inputs.contains_key("/custom/store/aaa-leaf.drv"));
    assert_eq!(
      drv_outputs(store_dir, &drv).expect("outputs")["out"],
      "/custom/store/bbb-hidden"
    );
  }
}
