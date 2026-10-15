//! Build log storage - captures and serves build logs

use std::{
  fs::Permissions,
  io::{self, ErrorKind},
  os::unix::fs::PermissionsExt,
  path::{Path, PathBuf},
};

use async_compression::tokio::{bufread::ZstdDecoder, write::ZstdEncoder};
use tempfile::{Builder, TempPath};
use tokio::{
  fs::{self, File},
  io::{AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader, copy},
};
use uuid::Uuid;

/// Prefix of logs still being written.
pub const PARTIAL_LOG_PREFIX: &str = ".partial-";

pub struct LogStorage {
  log_dir:  PathBuf,
  compress: bool,
}

impl LogStorage {
  /// Create a log storage instance. Creates the directory if needed.
  ///
  /// # Errors
  ///
  /// Returns error if directory creation fails.
  pub fn new(log_dir: PathBuf, compress: bool) -> io::Result<Self> {
    std::fs::create_dir_all(&log_dir)?;
    Ok(Self { log_dir, compress })
  }

  /// Returns the filesystem path where a completed build's log is stored.
  #[must_use]
  pub fn log_path(&self, build_id: &Uuid) -> PathBuf {
    let suffix = if self.compress { "log.zst" } else { "log" };
    self.log_dir.join(format!("{build_id}.{suffix}"))
  }

  /// Returns the filesystem path for an active (in-progress) build log.
  #[must_use]
  pub fn log_path_for_active(&self, build_id: &Uuid) -> PathBuf {
    self.log_dir.join(format!("{build_id}.active.log"))
  }

  /// Locate a completed log, including logs written before a config change.
  ///
  /// # Errors
  ///
  /// Returns errors checking file metadata other than a missing file.
  pub async fn find_log_path(
    &self,
    build_id: &Uuid,
  ) -> io::Result<Option<PathBuf>> {
    let preferred = self.log_path(build_id);
    let other = self.log_dir.join(format!(
      "{build_id}.{}",
      if self.compress { "log" } else { "log.zst" }
    ));
    for path in [preferred, other] {
      match fs::metadata(&path).await {
        Ok(_) => return Ok(Some(path)),
        Err(e) if e.kind() == ErrorKind::NotFound => {},
        Err(e) => return Err(e),
      }
    }
    Ok(None)
  }

  /// Write build output, publishing the completed log only after it is closed.
  ///
  /// # Errors
  ///
  /// Returns error if writing or publishing the log fails.
  pub async fn write_log(
    &self,
    build_id: &Uuid,
    stdout: &str,
    stderr: &str,
  ) -> io::Result<PathBuf> {
    let separator: &[u8] = if stdout.is_empty() || stderr.is_empty() {
      b""
    } else {
      b"\n"
    };
    let mut reader =
      stdout.as_bytes().chain(separator).chain(stderr.as_bytes());
    self.write_reader(build_id, &mut reader, None).await
  }

  /// Finalize an active log without buffering the entire log in memory.
  ///
  /// # Errors
  ///
  /// Returns error if reading or publishing the log fails.
  pub async fn finalize_log(&self, build_id: &Uuid) -> io::Result<PathBuf> {
    let active = self.log_path_for_active(build_id);
    if !self.compress {
      let path = self.log_path(build_id);
      fs::rename(active, &path).await?;
      self.remove_other_log(build_id).await;
      return Ok(path);
    }
    let mut reader = File::open(&active).await?;
    let permissions = reader.metadata().await?.permissions();
    let path = self
      .write_reader(build_id, &mut reader, Some(permissions))
      .await?;
    remove_published_log_source(&active).await;
    Ok(path)
  }

  async fn write_reader(
    &self,
    build_id: &Uuid,
    reader: &mut (impl AsyncRead + Unpin),
    permissions: Option<Permissions>,
  ) -> io::Result<PathBuf> {
    let path = self.log_path(build_id);
    // Existing logs stay private until their original permissions are restored.
    // New logs use ordinary creation permissions filtered by the service umask.
    let mode = if permissions.is_some() { 0o600 } else { 0o666 };
    let temporary = Builder::new()
      .prefix(PARTIAL_LOG_PREFIX)
      .permissions(Permissions::from_mode(mode))
      .tempfile_in(&self.log_dir)?;
    let (file, temporary) = temporary.into_parts();
    let file = File::from_std(file);
    if self.compress {
      let mut encoder = ZstdEncoder::new(file);
      copy(reader, &mut encoder).await?;
      encoder.shutdown().await?;
    } else {
      let mut file = file;
      copy(reader, &mut file).await?;
      file.shutdown().await?;
    }
    publish_log(temporary, &path, permissions).await?;
    self.remove_other_log(build_id).await;
    tracing::debug!(build_id = %build_id, path = %path.display(), "Wrote build log");
    Ok(path)
  }

  async fn remove_other_log(&self, build_id: &Uuid) {
    let suffix = if self.compress { "log" } else { "log.zst" };
    remove_published_log_source(
      &self.log_dir.join(format!("{build_id}.{suffix}")),
    )
    .await;
  }

  /// Read a build log from disk. Returns None if the file doesn't exist.
  ///
  /// # Errors
  ///
  /// Returns error if reading or decompressing the log fails.
  pub async fn read_log(&self, build_id: &Uuid) -> io::Result<Option<String>> {
    match self.find_log_path(build_id).await? {
      Some(path) => read_log_file(&path).await.map(Some),
      None => Ok(None),
    }
  }

  /// Delete both representations of a completed build log.
  ///
  /// # Errors
  ///
  /// Returns error if file deletion fails, other than a missing file.
  pub async fn delete_log(&self, build_id: &Uuid) -> io::Result<()> {
    for suffix in ["log", "log.zst"] {
      let path = self.log_dir.join(format!("{build_id}.{suffix}"));
      match fs::remove_file(path).await {
        Ok(()) => {},
        Err(e) if e.kind() == ErrorKind::NotFound => {},
        Err(e) => return Err(e),
      }
    }
    Ok(())
  }
}

async fn publish_log(
  temporary: TempPath,
  path: &Path,
  permissions: Option<Permissions>,
) -> io::Result<()> {
  if let Some(permissions) = permissions {
    fs::set_permissions(&temporary, permissions).await?;
  }
  temporary.persist(path).map_err(|e| e.error)?;
  Ok(())
}

// Publication has committed: cleanup errors must not hide the saved log.
async fn remove_published_log_source(path: &Path) {
  if let Err(error) = fs::remove_file(path).await
    && error.kind() != ErrorKind::NotFound
  {
    tracing::warn!(path = %path.display(), %error, "Failed to remove obsolete build log");
  }
}

/// Read a plain or zstd-compressed log at a caller-validated path.
///
/// # Errors
///
/// Returns error if reading or decompressing the log fails.
pub async fn read_log_file(path: &Path) -> io::Result<String> {
  let file = File::open(path).await?;
  let mut content = String::new();
  if path.extension().is_some_and(|extension| extension == "zst") {
    ZstdDecoder::new(BufReader::new(file))
      .read_to_string(&mut content)
      .await?;
  } else {
    BufReader::new(file).read_to_string(&mut content).await?;
  }
  Ok(content)
}
