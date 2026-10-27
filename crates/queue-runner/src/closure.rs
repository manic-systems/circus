use std::{num::NonZeroUsize, path::Path};

use circus_common::{
  PgPool,
  error::{CiError, Result},
  models::{PackageChange, PackageChangeKind},
  repo,
};
use dix::{
  DiffStatus,
  PackageDiff,
  Size,
  StorePath,
  StorePathInfo,
  StoreSnapshot,
  Version,
  VersionAmount,
  VersionDiff,
  diff_store_snapshots,
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
  let Some(previous) =
    repo::build_closure_diffs::latest_measured_success(pool, build_id).await?
  else {
    return Ok(());
  };

  let previous_outputs = repo::build_outputs::list_for_build(pool, previous)
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

  let report =
    diff_store_snapshots(&snapshot(&previous_closure)?, &snapshot(closure)?);
  let changes = report
    .diffs
    .into_iter()
    .filter_map(package_change)
    .collect::<Vec<PackageChange>>();
  repo::build_closure_diffs::upsert(pool, build_id, previous, &changes).await
}

fn snapshot(closure: &[ClosurePathInfo]) -> Result<StoreSnapshot> {
  let closure = closure
    .iter()
    .map(|info| {
      let path = StorePath::try_from(info.store_path.clone())
        .map_err(|error| CiError::Internal(error.to_string()))?;
      Ok(StorePathInfo {
        path,
        nar_size: Size::from_bytes(info.nar_size),
      })
    })
    .collect::<Result<Vec<StorePathInfo>>>()?;

  Ok(StoreSnapshot {
    closure,
    selected: Vec::new(),
  })
}

/// Size-only diffs and unversioned paths such as flake sources are skipped,
/// since only their size or count could change.
fn package_change(diff: PackageDiff) -> Option<PackageChange> {
  let change = diff.change?;
  let kind = match change.status {
    DiffStatus::Upgraded => PackageChangeKind::Upgraded,
    DiffStatus::Downgraded => PackageChangeKind::Downgraded,
    DiffStatus::Added => PackageChangeKind::Added,
    DiffStatus::Removed => PackageChangeKind::Removed,
    DiffStatus::Changed => PackageChangeKind::Changed,
    DiffStatus::Mixed => PackageChangeKind::Mixed,
  };

  let mut old = Vec::new();
  let mut new = Vec::new();
  for version in change.versions {
    match version {
      VersionDiff::Removed(removed) => old.push(versioned(&removed)?),
      VersionDiff::Added(added) => new.push(versioned(&added)?),
      VersionDiff::Changed {
        old: before,
        new: after,
      } => {
        old.push(versioned(&before)?);
        new.push(versioned(&after)?);
      },
      VersionDiff::AmountChanged {
        version,
        old_amount,
        new_amount,
      } => {
        let version = version?;
        old.push(counted(&version, old_amount));
        new.push(counted(&version, new_amount));
      },
    }
  }

  Some(PackageChange {
    name: diff.name,
    kind,
    old,
    new,
  })
}

fn versioned(amount: &VersionAmount) -> Option<String> {
  Some(counted(amount.version.as_ref()?, amount.amount))
}

fn counted(version: &Version, amount: NonZeroUsize) -> String {
  match amount.get() {
    1 => version.to_string(),
    count => format!("{version} ×{count}"),
  }
}

async fn path_infos(output_paths: &[String]) -> Option<Vec<ClosurePathInfo>> {
  if output_paths.is_empty() {
    return None;
  }
  get_recursive_path_infos_with_nix(Path::new("nix"), output_paths).await
}
