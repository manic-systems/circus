use std::path::Path;

use circus_common::{
  PgPool,
  error::Result,
  models::{BuildStatus, PackageChange, PackageChangeKind},
  repo,
};
use dix_diff::{
  Diff,
  DiffStatus,
  Package,
  PackageSnapshot,
  VersionAmount,
  VersionDiff,
  diff_snapshots,
};
use uuid::Uuid;

use crate::{
  dispatch::invalid_store_paths,
  worker::{ClosurePathInfo, get_recursive_path_infos_with_nix},
};

pub async fn record(pool: &PgPool, build_id: Uuid, output_paths: &[String]) {
  let Some(closure) = path_infos(output_paths).await else {
    return;
  };

  let size = closure.iter().map(|info| info.nar_size).sum();
  if let Err(error) = repo::builds::set_closure_size(pool, build_id, size).await
  {
    tracing::warn!(%build_id, "Failed to record closure size: {error}");
  }

  if let Err(error) = record_package_changes(pool, build_id, &closure).await {
    tracing::warn!(%build_id, "Failed to diff the closure: {error}");
  }
}

async fn record_package_changes(
  pool: &PgPool,
  build_id: Uuid,
  closure: &[ClosurePathInfo],
) -> Result<()> {
  let history = repo::builds::job_history(pool, build_id, 20).await?;
  // Dedup successes have no output rows, so compare against a measured build.
  let Some(previous) = history.iter().find(|past| {
    past.status == BuildStatus::Succeeded && past.closure_size.is_some()
  }) else {
    return Ok(());
  };

  let previous_outputs =
    repo::build_outputs::list_for_build(pool, previous.build_id)
      .await?
      .into_iter()
      .filter_map(|output| output.path)
      .collect::<Vec<String>>();
  let garbage_collected = invalid_store_paths(&previous_outputs)
    .await
    .is_ok_and(|missing| !missing.is_empty());
  if garbage_collected {
    return Ok(());
  }
  let Some(previous_closure) = path_infos(&previous_outputs).await else {
    return Ok(());
  };

  let changes =
    diff_snapshots(&snapshot(&previous_closure), &snapshot(closure))
      .into_iter()
      .map(package_change)
      .collect::<Vec<PackageChange>>();
  repo::build_closure_diffs::upsert(pool, build_id, previous.build_id, &changes)
    .await
}

fn snapshot(closure: &[ClosurePathInfo]) -> PackageSnapshot {
  PackageSnapshot::new(
    closure
      .iter()
      .filter_map(|info| package(&info.store_path))
      .collect::<Vec<Package>>(),
  )
}

/// Split a store path the way `builtins.parseDrvName` does, where the version
/// starts at the first dash followed by a digit. Unversioned paths such as
/// flake sources are skipped, since only their count could change.
fn package(store_path: &str) -> Option<Package> {
  let (_hash, name) = store_path.rsplit('/').next()?.split_once('-')?;
  let split = name.match_indices('-').find(|(index, _)| {
    name[index + 1..]
      .chars()
      .next()
      .is_some_and(|first| first.is_ascii_digit())
  });
  let (index, _) = split?;
  Some(Package::new(&name[..index], &name[index + 1..]))
}

fn package_change(diff: Diff) -> PackageChange {
  let kind = match diff.status {
    DiffStatus::Upgraded => PackageChangeKind::Upgraded,
    DiffStatus::Downgraded => PackageChangeKind::Downgraded,
    DiffStatus::Added => PackageChangeKind::Added,
    DiffStatus::Removed => PackageChangeKind::Removed,
    DiffStatus::Changed => PackageChangeKind::Changed,
    DiffStatus::Mixed => PackageChangeKind::Mixed,
  };

  let mut old = Vec::new();
  let mut new = Vec::new();
  for version in diff.versions {
    match version {
      VersionDiff::Removed(removed) => old.push(counted(&removed)),
      VersionDiff::Added(added) => new.push(counted(&added)),
      VersionDiff::Changed {
        old: before,
        new: after,
      } => {
        old.push(counted(&before));
        new.push(counted(&after));
      },
      VersionDiff::AmountChanged {
        version,
        old_amount,
        new_amount,
      } => {
        old.push(counted(&VersionAmount::new(version.clone(), old_amount)));
        new.push(counted(&VersionAmount::new(version, new_amount)));
      },
    }
  }

  PackageChange {
    name: diff.name,
    kind,
    old,
    new,
  }
}

fn counted(amount: &VersionAmount) -> String {
  match amount.amount.get() {
    1 => amount.version.to_string(),
    count => format!("{} ×{count}", amount.version),
  }
}

async fn path_infos(output_paths: &[String]) -> Option<Vec<ClosurePathInfo>> {
  if output_paths.is_empty() {
    return None;
  }
  get_recursive_path_infos_with_nix(Path::new("nix"), output_paths).await
}
