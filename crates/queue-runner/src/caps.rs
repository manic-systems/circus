//! What the runner host itself can execute, gating the local-build fallback
//! the same way agent `supported_features` gate dispatch.

use circus_common::{models::builds_system, repo::builds::BuildVenue};

#[derive(Debug)]
pub struct RunnerCaps {
  enabled:  bool,
  systems:  Vec<String>,
  features: Vec<String>,
}

async fn nix_config_show(setting: &str) -> Option<String> {
  let out = tokio::process::Command::new("nix")
    .args([
      "--extra-experimental-features",
      "nix-command",
      "config",
      "show",
      setting,
    ])
    .output()
    .await
    .ok()?;
  if !out.status.success() {
    return None;
  }
  Some(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

impl RunnerCaps {
  #[must_use]
  pub const fn new(
    enabled: bool,
    systems: Vec<String>,
    features: Vec<String>,
  ) -> Self {
    Self {
      enabled,
      systems,
      features,
    }
  }

  /// Resolve from config overrides, else the host nix's own settings.
  pub async fn detect(
    enabled: bool,
    systems: Option<Vec<String>>,
    features: Option<Vec<String>>,
  ) -> Self {
    let systems = if let Some(systems) = systems {
      systems
    } else {
      let native = nix_config_show("system").await;
      if native.is_none() {
        tracing::error!(
          "could not detect the native nix system; local builds may not be \
           scheduled (set queue_runner.local_systems to override)"
        );
      }
      let mut detected = native.into_iter().collect::<Vec<String>>();
      if let Some(extra) = nix_config_show("extra-platforms").await {
        detected.extend(extra.split_whitespace().map(str::to_owned));
      }
      detected
    };
    let features = if let Some(features) = features {
      features
    } else {
      nix_config_show("system-features").await.map_or_else(
        || {
          tracing::error!(
            "could not detect local nix system-features; only featureless \
             builds will run locally (set queue_runner.local_features to \
             override)"
          );
          Vec::new()
        },
        |raw| raw.split_whitespace().map(str::to_owned).collect(),
      )
    };
    Self::new(enabled, systems, features)
  }

  #[must_use]
  pub const fn enabled(&self) -> bool {
    self.enabled
  }

  /// The runner host as a [`list_pending`] venue, if local builds are on.
  ///
  /// [`list_pending`]: circus_common::repo::builds::list_pending
  #[must_use]
  pub fn venue(&self) -> Option<BuildVenue<'_>> {
    self.enabled.then(|| {
      BuildVenue {
        systems:            &self.systems,
        supported_features: &self.features,
        mandatory_features: &[],
        runs_systemless:    true,
      }
    })
  }

  /// Whether the runner host can run a build for `system` demanding `features`.
  #[must_use]
  pub fn supports(&self, system: Option<&str>, features: &[String]) -> bool {
    self.enabled
      && system.is_none_or(|s| builds_system(&self.systems, s))
      && features.iter().all(|f| self.features.contains(f))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn supports_checks_system_features_and_enablement() {
    let caps = RunnerCaps::new(true, vec!["x86_64-linux".into()], vec![
      "kvm".into(),
      "nixos-test".into(),
    ]);
    assert!(caps.supports(Some("x86_64-linux"), &["kvm".into()]));
    assert!(caps.supports(None, &[]));
    assert!(!caps.supports(Some("aarch64-linux"), &[]));
    assert!(!caps.supports(Some("x86_64-linux"), &["uid-range".into()]));

    let disabled = RunnerCaps::new(false, vec!["x86_64-linux".into()], vec![]);
    assert!(!disabled.supports(Some("x86_64-linux"), &[]));
  }
}
