//! Error types for circus

use thiserror::Error;

use crate::validation::ValidationError;

#[derive(Error, Debug)]
pub enum CiError {
  #[error("Database error: {0}")]
  Database(#[from] sqlx::Error),

  #[error("Git error: {0}")]
  Git(#[from] git2::Error),

  #[error("Serialization error: {0}")]
  Serialization(#[from] serde_json::Error),

  #[error("IO error: {0}")]
  Io(#[from] std::io::Error),

  #[error("Configuration error: {0}")]
  Config(String),

  #[error("Build error: {0}")]
  Build(String),

  #[error("Not found: {0}")]
  NotFound(String),

  #[error("Validation error: {0}")]
  Validation(String),

  #[error("Conflict: {0}")]
  Conflict(String),

  #[error("Timeout: {0}")]
  Timeout(String),

  #[error("Nix evaluation error: {0}")]
  NixEval(String),

  #[error("Disk space error: {0}")]
  DiskSpace(String),

  #[error("Unauthorized: {0}")]
  Unauthorized(String),

  #[error("Forbidden: {0}")]
  Forbidden(String),

  #[error("Internal error: {0}")]
  Internal(String),
}

impl CiError {
  /// Check if this error indicates a disk-full condition.
  #[must_use]
  pub fn is_disk_full(&self) -> bool {
    let msg = self.to_string().to_lowercase();
    msg.contains("no space left on device")
      || msg.contains("disk full")
      || msg.contains("enospc")
      || msg.contains("cannot create directory")
      || msg.contains("sqlite.*busy")
  }
}

pub type Result<T> = std::result::Result<T, CiError>;

impl From<ValidationError> for CiError {
  fn from(error: ValidationError) -> Self {
    Self::Validation(error.to_string())
  }
}

pub trait SqlxResultExt<T> {
  /// # Errors
  ///
  /// Returns `CiError::Conflict` for unique-constraint violations and
  /// `CiError::Database` for other `SQLx` errors.
  fn on_unique_violation(self, msg: impl FnOnce() -> String) -> Result<T>;
}

impl<T> SqlxResultExt<T> for std::result::Result<T, sqlx::Error> {
  fn on_unique_violation(self, msg: impl FnOnce() -> String) -> Result<T> {
    self.map_err(|e| {
      match &e {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
          CiError::Conflict(msg())
        },
        _ => CiError::Database(e),
      }
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn sqlx_result_ext_preserves_non_unique_errors() {
    let result = Err::<(), _>(sqlx::Error::RowNotFound)
      .on_unique_violation(|| "conflict".to_string());

    assert!(matches!(
      result,
      Err(CiError::Database(sqlx::Error::RowNotFound))
    ));
  }
}

/// Check disk space on the given path
///
/// # Errors
///
/// Returns error if statfs call fails or path is invalid.
pub fn check_disk_space(path: &std::path::Path) -> Result<DiskSpaceInfo> {
  fn to_gb(bytes: u64) -> f64 {
    bytes as f64 / 1024.0 / 1024.0 / 1024.0
  }

  #[cfg(unix)]
  {
    let stat = nix::sys::statvfs::statvfs(path)
      .map_err(|e| CiError::Io(std::io::Error::from_raw_os_error(e as i32)))?;
    let block_size = stat.fragment_size();
    let bavail = stat.blocks_available().saturating_mul(block_size);
    let bfree = stat.blocks_free().saturating_mul(block_size);
    let btotal = stat.blocks().saturating_mul(block_size);

    Ok(DiskSpaceInfo {
      total_gb:     to_gb(btotal),
      free_gb:      to_gb(bfree),
      available_gb: to_gb(bavail),
      percent_used: if btotal > 0 {
        ((btotal - bfree) as f64 / btotal as f64) * 100.0
      } else {
        0.0
      },
    })
  }

  #[cfg(not(unix))]
  {
    let _ = path;
    Err(CiError::Io(std::io::Error::new(
      std::io::ErrorKind::Other,
      "Disk space check not implemented for this platform",
    )))
  }
}

/// Disk space information
#[derive(Debug, Clone)]
pub struct DiskSpaceInfo {
  pub total_gb:     f64,
  pub free_gb:      f64,
  pub available_gb: f64,
  pub percent_used: f64,
}

impl DiskSpaceInfo {
  /// Check if disk space is critically low (less than 1GB available)
  #[must_use]
  pub fn is_critical(&self) -> bool {
    self.available_gb < 1.0
  }

  /// Check if disk space is low (less than 5GB available)
  #[must_use]
  pub fn is_low(&self) -> bool {
    self.available_gb < 5.0
  }

  /// Get a human-readable summary
  #[must_use]
  pub fn summary(&self) -> String {
    format!(
      "Total: {:.1}GB, Free: {:.1}GB ({:.1}%), Available: {:.1}GB",
      self.total_gb, self.free_gb, self.percent_used, self.available_gb
    )
  }
}
