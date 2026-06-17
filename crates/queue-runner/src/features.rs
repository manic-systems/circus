//! Dispatch-time effective features
//!
//! The venue realizes the whole unbuilt closure, not just the job drv, so union
//! `requiredSystemFeatures` over what nix says will actually be built.

use std::{collections::BTreeSet, path::Path};

use circus_common::{models::Build, repo};
use sqlx::PgPool;
use tokio::{process::Command, sync::Semaphore};

/// Cap dry-run fan-out on a cold queue.
static COMPUTE_SLOTS: Semaphore = Semaphore::const_new(4);

/// Parse the will-build list from `nix-store --realise --dry-run` stderr.
fn parse_dry_run_will_build(stderr: &str) -> Vec<String> {
  let mut out = Vec::new();
  let mut in_built_section = false;
  for line in stderr.lines() {
    if line.starts_with(' ') || line.starts_with('\t') {
      if in_built_section {
        let path = line.trim();
        if Path::new(path)
          .extension()
          .is_some_and(|ext| ext.eq_ignore_ascii_case("drv"))
        {
          out.push(path.to_owned());
        }
      }
      continue;
    }
    in_built_section = line.contains("will be built");
  }
  out
}

async fn will_build_set(drv_path: &str) -> color_eyre::Result<Vec<String>> {
  let out = Command::new("nix-store")
    .args(["--realise", "--dry-run", drv_path])
    .output()
    .await?;
  if !out.status.success() {
    color_eyre::eyre::bail!(
      "nix-store --realise --dry-run failed: {}",
      String::from_utf8_lossy(&out.stderr).trim()
    );
  }
  Ok(parse_dry_run_will_build(&String::from_utf8_lossy(
    &out.stderr,
  )))
}

/// Union `requiredSystemFeatures` over the drvs the runner's nix says will
/// be built for `drv_path` given current store and substituter state.
async fn compute(drv_path: &str) -> color_eyre::Result<Vec<String>> {
  let drvs = will_build_set(drv_path).await?;
  Ok(circus_nix::derivation::show_required_features(&drvs).await?)
}

/// Floor the computed set to the job-level features so a silent dry-run
/// parse miss can never erase the drv's own constraints.
fn with_required_floor(
  computed: Vec<String>,
  required: &[String],
) -> Vec<String> {
  computed
    .into_iter()
    .chain(required.iter().cloned())
    .collect::<BTreeSet<_>>()
    .into_iter()
    .collect()
}

/// Populate `effective_features` on the first dispatch attempt, and on failure
/// fall back to the job-level `required_features`.
///
/// # Panics
///
/// Only if the compute semaphore has been closed, which never happens.
pub async fn ensure_effective_features(
  pool: &PgPool,
  mut build: Build,
) -> Build {
  if build.effective_features.is_some() {
    return build;
  }
  #[expect(
    clippy::expect_used,
    reason = "the static semaphore is never closed, so acquire never errors"
  )]
  let _slot = COMPUTE_SLOTS
    .acquire()
    .await
    .expect("compute semaphore is never closed");
  match compute(&build.drv_path).await {
    Ok(computed) => {
      let features = with_required_floor(computed, &build.required_features);
      if let Err(e) =
        repo::builds::set_effective_features(pool, build.id, &features).await
      {
        tracing::warn!(
          build_id = %build.id,
          "failed to persist effective_features: {e}"
        );
      }
      tracing::debug!(
        build_id = %build.id,
        ?features,
        "computed effective features"
      );
      build.effective_features = Some(features);
    },
    Err(e) => {
      tracing::warn!(
        build_id = %build.id,
        drv = %build.drv_path,
        "effective feature computation failed, scheduling on job-level \
         required_features: {e}"
      );
    },
  }
  build
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn collects_built_drvs_under_singular_and_plural_headers() {
    let plural = "these 2 derivations will be built:\n  \
                  /nix/store/aaa-inner.drv\n  /nix/store/bbb-outer.drv\n";
    assert_eq!(parse_dry_run_will_build(plural), vec![
      "/nix/store/aaa-inner.drv",
      "/nix/store/bbb-outer.drv"
    ]);

    let singular =
      "this derivation will be built:\n  /nix/store/ccc-single.drv\n";
    assert_eq!(parse_dry_run_will_build(singular), vec![
      "/nix/store/ccc-single.drv"
    ]);
  }

  #[test]
  fn floor_keeps_job_features_when_computed_is_empty() {
    assert_eq!(
      with_required_floor(Vec::new(), &["kvm".into(), "uid-range".into()]),
      vec!["kvm", "uid-range"]
    );
  }

  #[test]
  fn ignores_fetched_section_even_for_drv_paths() {
    let stderr = "these 1 derivations will be built:\n  \
                  /nix/store/aaa-built.drv\nthese 2 paths will be fetched \
                  (1.0 MiB download):\n  /nix/store/bbb-out\n  \
                  /nix/store/ccc-substituted.drv\n";
    assert_eq!(parse_dry_run_will_build(stderr), vec![
      "/nix/store/aaa-built.drv"
    ]);
  }
}
