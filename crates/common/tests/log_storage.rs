use std::{fs::Permissions, io, os::unix::fs::PermissionsExt};

use circus_common::log_storage::{LogStorage, read_log_file};
use tempfile::tempdir;
use tokio::{
  fs::{self, File},
  io::AsyncReadExt,
};
use uuid::Uuid;

#[tokio::test]
async fn logs_survive_compression_setting_changes() -> io::Result<()> {
  let dir = tempdir()?;
  let plain = LogStorage::new(dir.path().to_path_buf(), false)?;
  let compressed = LogStorage::new(dir.path().to_path_buf(), true)?;
  let id = Uuid::new_v4();

  plain.write_log(&id, "old output", "").await?;
  assert_eq!(
    compressed.read_log(&id).await?.as_deref(),
    Some("old output")
  );

  compressed.write_log(&id, "new output λ", "failure").await?;
  assert_eq!(
    plain.read_log(&id).await?.as_deref(),
    Some("new output λ\nfailure")
  );
  assert!(!plain.log_path(&id).exists());

  plain.write_log(&id, "", "error only").await?;
  assert_eq!(
    compressed.read_log(&id).await?.as_deref(),
    Some("error only")
  );
  assert!(!compressed.log_path(&id).exists());

  compressed.delete_log(&id).await?;
  assert_eq!(plain.read_log(&id).await?, None);

  compressed.write_log(&id, "", "").await?;
  assert_eq!(plain.read_log(&id).await?.as_deref(), Some(""));

  plain.delete_log(&id).await?;
  assert_eq!(compressed.read_log(&id).await?, None);

  Ok(())
}

#[tokio::test]
async fn finalizing_preserves_output_and_open_live_readers() -> io::Result<()> {
  let dir = tempdir()?;

  for compress in [false, true] {
    let storage = LogStorage::new(dir.path().to_path_buf(), compress)?;
    let id = Uuid::new_v4();
    let active = storage.log_path_for_active(&id);
    let output = "building λ\ncompiler output\n".repeat(8192);

    fs::write(&active, &output).await?;
    fs::set_permissions(&active, Permissions::from_mode(0o600)).await?;
    let mut live_reader = File::open(&active).await?;

    let final_path = storage.finalize_log(&id).await?;
    assert!(!active.exists());
    assert_eq!(
      fs::metadata(&final_path).await?.permissions().mode() & 0o777,
      0o600,
    );
    assert_eq!(read_log_file(&final_path).await?, output);

    let mut remaining = String::new();
    live_reader.read_to_string(&mut remaining).await?;
    assert_eq!(remaining, output);
  }

  Ok(())
}

#[tokio::test]
async fn failed_publication_preserves_active_log() -> io::Result<()> {
  let dir = tempdir()?;
  let storage = LogStorage::new(dir.path().to_path_buf(), true)?;
  let id = Uuid::new_v4();
  let active = storage.log_path_for_active(&id);

  fs::write(&active, "keep this output").await?;
  fs::create_dir(storage.log_path(&id)).await?;

  assert!(storage.finalize_log(&id).await.is_err());
  assert_eq!(fs::read_to_string(active).await?, "keep this output");

  // Only the original active log and the conflicting destination remain.
  let mut entries = fs::read_dir(dir.path()).await?;
  let mut count = 0;

  while entries.next_entry().await?.is_some() {
    count += 1;
  }

  assert_eq!(count, 2);

  Ok(())
}
