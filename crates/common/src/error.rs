//! Error types for circus

use thiserror::Error;

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
    use std::{ffi::CString, os::unix::ffi::OsStrExt};

    let cpath = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
      CiError::DiskSpace("Invalid path for disk check".to_string())
    })?;
    let mut statfs: libc::statfs = unsafe { std::mem::zeroed() };

    if unsafe { libc::statfs(cpath.as_ptr(), &raw mut statfs) } != 0 {
      return Err(CiError::Io(std::io::Error::last_os_error()));
    }

    let bavail = statfs.f_bavail * statfs.f_bsize.cast_unsigned();
    let bfree = statfs.f_bfree * statfs.f_bsize.cast_unsigned();
    let btotal = statfs.f_blocks * statfs.f_bsize.cast_unsigned();

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
    let available = fs_available_space(path)?;
    Ok(DiskSpaceInfo {
      total_gb:     0.0,
      free_gb:      to_gb(available),
      available_gb: to_gb(available),
      percent_used: 0.0,
    })
  }
}

#[cfg(not(unix))]
fn fs_available_space(path: &std::path::Path) -> Result<u64> {
  use std::io::Read;

  let metadata = std::fs::metadata(path)?;
  let volume = path.to_path_buf();
  if let Some(parent) = path.parent() {
    let volume = if path.is_file() {
      parent.to_path_buf()
    } else {
      volume
    };
    #[cfg(windows)]
    {
      let vol = widestring::WideCString::from_os_str(&volume).map_err(|e| {
        CiError::Io(std::io::Error::new(std::io::ErrorKind::Other, e))
      })?;
      let mut lp_sz_path: [u16; 261] = [0; 261];
      for (i, c) in
        std::os::windows::ffi::OsStrExt::encode_wide(&vol).enumerate()
      {
        if i < 261 {
          lp_sz_path[i] = c;
        }
      }
      let mut lp_free_bytes: u64 = 0;
      let mut lp_total_bytes: u64 = 0;
      let lp_sectors_per_cluster: u64 = 0;
      let lp_bytes_per_sector: u64 = 0;
      unsafe {
        GetDiskFreeSpaceW(
          lp_sz_path.as_ptr(),
          &mut lp_sectors_per_cluster as *mut _ as *mut _,
          &mut lp_bytes_per_sector as *mut _ as *mut _,
          &mut lp_free_bytes,
          &mut lp_total_bytes,
        );
      }
      Ok(lp_free_bytes)
    }
    #[cfg(not(windows))]
    Err(CiError::Io(std::io::Error::new(
      std::io::ErrorKind::Other,
      "Disk space check not implemented for this platform",
    )))
  } else {
    Err(CiError::Io(std::io::Error::new(
      std::io::ErrorKind::Other,
      "Cannot determine parent path",
    )))
  }
}

#[cfg(windows)]
extern "system" {
  fn GetDiskFreeSpaceW(
    lp_root_path_name: *const u16,
    lp_sectors_per_cluster: *mut u64,
    lp_bytes_per_sector: *mut u64,
    lp_free_bytes_available_to_caller: *mut u64,
    lp_total_number_of_bytes: *mut u64,
  ) -> i32;
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
