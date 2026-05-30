//! GC root management - prevents nix-store --gc from deleting build outputs

use std::{
  collections::HashSet,
  os::unix::fs::symlink,
  path::{Path, PathBuf},
  time::Duration,
};

use tracing::{debug, info, warn};
use uuid::Uuid;

/// Remove GC root symlinks with mtime older than `max_age`. Returns count
/// removed. Symlinks whose filename matches a UUID in `pinned_build_ids` are
/// skipped regardless of age.
///
/// # Errors
///
/// Returns error if directory read fails.
pub fn cleanup_old_roots<S: std::hash::BuildHasher>(
  roots_dir: &Path,
  max_age: Duration,
  pinned_build_ids: &HashSet<Uuid, S>,
) -> std::io::Result<u64> {
  if !roots_dir.exists() {
    return Ok(0);
  }

  let mut count = 0u64;
  let now = std::time::SystemTime::now();

  for entry in std::fs::read_dir(roots_dir)? {
    let entry = entry?;

    // Check if this root is pinned (filename is a build UUID with keep=true)
    if let Some(name) = entry.file_name().to_str()
      && let Ok(build_id) = name.parse::<Uuid>()
      && pinned_build_ids.contains(&build_id)
    {
      debug!(build_id = %build_id, "Skipping pinned GC root");
      continue;
    }

    let Ok(metadata) = entry.metadata() else {
      continue;
    };

    let Ok(modified) = metadata.modified() else {
      continue;
    };

    if let Ok(age) = now.duration_since(modified)
      && age > max_age
    {
      if let Err(e) = std::fs::remove_file(entry.path()) {
        warn!(
          "Failed to remove old GC root {}: {e}",
          entry.path().display()
        );
      } else {
        count += 1;
      }
    }
  }

  Ok(count)
}

pub struct GcRoots {
  roots_dir: PathBuf,
  store_dir: PathBuf,
  enabled:   bool,
}

impl GcRoots {
  /// Create a GC roots manager. Creates the directory if enabled.
  ///
  /// # Errors
  ///
  /// Returns error if directory creation or permission setting fails.
  pub fn new(
    roots_dir: PathBuf,
    store_dir: PathBuf,
    enabled: bool,
  ) -> std::io::Result<Self> {
    if enabled {
      std::fs::create_dir_all(&roots_dir)?;
      #[cfg(unix)]
      {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
          &roots_dir,
          std::fs::Permissions::from_mode(0o700),
        )?;
      }
    }
    Ok(Self {
      roots_dir,
      store_dir,
      enabled,
    })
  }

  /// Register a GC root for a build output. Returns the symlink path.
  ///
  /// # Errors
  ///
  /// Returns error if path is invalid or symlink creation fails.
  pub fn register(
    &self,
    build_id: &uuid::Uuid,
    output_path: &str,
  ) -> std::io::Result<Option<PathBuf>> {
    if !self.enabled {
      return Ok(None);
    }
    if !crate::validate::is_valid_store_path(
      output_path,
      &self.store_dir.to_string_lossy(),
    ) {
      return Err(std::io::Error::new(
        std::io::ErrorKind::InvalidInput,
        format!("Invalid store path: {output_path}"),
      ));
    }
    let link_path = self.roots_dir.join(build_id.to_string());
    // Remove existing symlink if present
    if link_path.exists() || link_path.symlink_metadata().is_ok() {
      std::fs::remove_file(&link_path)?;
    }
    symlink(output_path, &link_path)?;
    info!(build_id = %build_id, output = output_path, "Registered GC root");
    Ok(Some(link_path))
  }

  /// Remove a GC root for a build.
  pub fn remove(&self, build_id: &uuid::Uuid) {
    if !self.enabled {
      return;
    }
    let link_path = self.roots_dir.join(build_id.to_string());
    if let Err(e) = std::fs::remove_file(&link_path) {
      if e.kind() != std::io::ErrorKind::NotFound {
        warn!(build_id = %build_id, "Failed to remove GC root: {e}");
      }
    } else {
      info!(build_id = %build_id, "Removed GC root");
    }
  }
}
