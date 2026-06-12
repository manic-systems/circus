//! Build log storage - captures and serves build logs

use std::path::PathBuf;

use uuid::Uuid;

pub struct LogStorage {
  log_dir: PathBuf,
}

impl LogStorage {
  /// Create a log storage instance. Creates the directory if needed.
  ///
  /// # Errors
  ///
  /// Returns error if directory creation fails.
  pub fn new(log_dir: PathBuf) -> std::io::Result<Self> {
    std::fs::create_dir_all(&log_dir)?;
    Ok(Self { log_dir })
  }

  /// # Returns
  ///
  /// Returns the filesystem path where a build's log should be stored
  #[must_use]
  pub fn log_path(&self, build_id: &Uuid) -> PathBuf {
    self.log_dir.join(format!("{build_id}.log"))
  }

  /// # Returns
  ///
  /// Returns the filesystem path for an active (in-progress) build log
  #[must_use]
  pub fn log_path_for_active(&self, build_id: &Uuid) -> PathBuf {
    self.log_dir.join(format!("{build_id}.active.log"))
  }

  /// Write build log content to file
  ///
  /// # Errors
  ///
  /// Returns error if file write fails.
  pub fn write_log(
    &self,
    build_id: &Uuid,
    stdout: &str,
    stderr: &str,
  ) -> std::io::Result<PathBuf> {
    let path = self.log_path(build_id);
    let mut content = String::new();
    if !stdout.is_empty() {
      content.push_str(stdout);
    }
    if !stderr.is_empty() {
      if !content.is_empty() {
        content.push('\n');
      }
      content.push_str(stderr);
    }
    std::fs::write(&path, &content)?;
    tracing::debug!(build_id = %build_id, path = %path.display(), "Wrote build log");
    Ok(path)
  }

  /// Read a build log from disk. Returns None if the file doesn't exist.
  ///
  /// # Errors
  ///
  /// Returns error if file read fails.
  pub fn read_log(&self, build_id: &Uuid) -> std::io::Result<Option<String>> {
    let path = self.log_path(build_id);
    if !path.exists() {
      return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    Ok(Some(content))
  }

  /// Delete a build log
  ///
  /// # Errors
  ///
  /// Returns error if file deletion fails.
  pub fn delete_log(&self, build_id: &Uuid) -> std::io::Result<()> {
    let path = self.log_path(build_id);
    if path.exists() {
      std::fs::remove_file(&path)?;
    }
    Ok(())
  }
}
