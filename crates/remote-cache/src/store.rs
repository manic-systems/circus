//! One content-addressed directory with a byte budget. Objects live at
//! `<dir>/<first two hash digits>/<key>`, written to `<dir>/tmp` and renamed
//! into place, and the least recently used are evicted past the budget. File
//! mtimes carry recency across restarts.

use std::{
  collections::{BTreeMap, HashMap},
  fs,
  io,
  path::PathBuf,
  sync::Mutex,
  time::{Duration, SystemTime},
};

use tokio::fs::File;

use crate::digest::Key;

/// Reads within this long of the last refresh leave the mtime alone.
const TOUCH_INTERVAL: Duration = Duration::from_secs(600);

pub struct Store {
  dir:    PathBuf,
  tmp:    PathBuf,
  budget: u64,
  index:  Mutex<Index>,
}

#[derive(Default)]
struct Index {
  entries:       HashMap<Key, Entry>,
  order:         BTreeMap<u64, Key>,
  total:         u64,
  clock:         u64,
  evictions:     u64,
  evicted_bytes: u64,
}

/// What a store holds and has evicted, for metrics.
pub struct Stats {
  pub bytes:         u64,
  pub objects:       u64,
  pub evictions:     u64,
  pub evicted_bytes: u64,
}

struct Entry {
  size: u64,
  tick: u64,
}

impl Index {
  fn insert(&mut self, key: Key, size: u64) {
    self.remove(key);

    self.clock += 1;
    self.order.insert(self.clock, key);

    self.entries.insert(key, Entry {
      size,
      tick: self.clock,
    });

    self.total += size;
  }

  fn remove(&mut self, key: Key) {
    if let Some(entry) = self.entries.remove(&key) {
      self.order.remove(&entry.tick);
      self.total -= entry.size;
    }
  }

  fn touch(&mut self, key: Key) {
    if let Some(entry) = self.entries.get_mut(&key) {
      self.order.remove(&entry.tick);
      self.clock += 1;
      entry.tick = self.clock;
      self.order.insert(self.clock, key);
    }
  }

  /// Drops least recently used entries until `budget` fits, returning their
  /// keys.
  fn shrink(&mut self, budget: u64) -> Vec<Key> {
    let mut evicted = Vec::new();

    while self.total > budget
      && let Some((_, key)) = self.order.pop_first()
    {
      if let Some(entry) = self.entries.remove(&key) {
        self.total -= entry.size;
        self.evictions += 1;
        self.evicted_bytes += entry.size;
      }
      evicted.push(key);
    }

    evicted
  }
}

impl Store {
  /// Opens `dir`, discarding unfinished uploads and indexing what is there.
  pub fn open(dir: PathBuf, budget: u64) -> io::Result<Self> {
    let tmp = dir.join("tmp");
    if let Err(error) = fs::remove_dir_all(&tmp)
      && error.kind() != io::ErrorKind::NotFound
    {
      return Err(error);
    }

    fs::create_dir_all(&tmp)?;

    let mut found = Vec::new();

    for shard in fs::read_dir(&dir)? {
      let shard = shard?;
      if shard.file_name() == "tmp" || !shard.file_type()?.is_dir() {
        continue;
      }

      for object in fs::read_dir(shard.path())? {
        let object = object?;
        let Some(Ok(key)) = object.file_name().to_str().map(str::parse::<Key>)
        else {
          continue;
        };

        let metadata = object.metadata()?;
        found.push((metadata.modified()?, key, metadata.len()));
      }
    }

    found.sort_unstable_by_key(|(modified, ..)| *modified);

    let mut index = Index::default();
    for (_, key, size) in found {
      index.insert(key, size);
    }

    let store = Self {
      dir,
      tmp,
      budget,
      index: Mutex::new(index),
    };

    let evicted = store.lock().shrink(budget);
    store.delete(&evicted);
    Ok(store)
  }

  fn lock(&self) -> std::sync::MutexGuard<'_, Index> {
    self
      .index
      .lock()
      .unwrap_or_else(std::sync::PoisonError::into_inner)
  }

  pub const fn budget(&self) -> u64 {
    self.budget
  }

  fn path(&self, key: Key) -> PathBuf {
    self.dir.join(key.hash.shard()).join(key.to_string())
  }

  /// Marks `key` used, as a client asking whether it exists will want it.
  pub fn touch(&self, key: Key) {
    self.lock().touch(key);
  }

  /// The stored size of `key`, if it is present.
  pub fn size(&self, key: Key) -> Option<u64> {
    self.lock().entries.get(&key).map(|entry| entry.size)
  }

  pub fn stats(&self) -> Stats {
    let index = self.lock();
    Stats {
      bytes:         index.total,
      objects:       index.entries.len() as u64,
      evictions:     index.evictions,
      evicted_bytes: index.evicted_bytes,
    }
  }

  /// Opens `key` for reading and marks it used. A file that vanished under
  /// the index reads as absent.
  pub async fn open_object(&self, key: Key) -> io::Result<Option<File>> {
    let path = self.path(key);
    match File::open(&path).await {
      Ok(file) => {
        self.lock().touch(key);
        refresh_mtime(&file).await;
        Ok(Some(file))
      },
      Err(error) if error.kind() == io::ErrorKind::NotFound => {
        self.lock().remove(key);
        Ok(None)
      },
      Err(error) => Err(error),
    }
  }

  /// The contents of `key`, marking it used.
  pub async fn read(&self, key: Key) -> io::Result<Option<Vec<u8>>> {
    let Some(mut file) = self.open_object(key).await? else {
      return Ok(None);
    };

    let mut bytes = Vec::new();
    tokio::io::AsyncReadExt::read_to_end(&mut file, &mut bytes).await?;
    Ok(Some(bytes))
  }

  /// A fresh file under `tmp` for an upload, removed on drop unless
  /// committed.
  pub async fn upload(&self) -> io::Result<Upload> {
    let path = self.tmp.join(uuid::Uuid::new_v4().simple().to_string());
    let file = File::create_new(&path).await?;
    Ok(Upload {
      path: Some(path),
      file,
    })
  }

  /// Moves a finished upload into place as `key`, then evicts down to the
  /// budget.
  pub async fn commit(
    &self,
    mut upload: Upload,
    key: Key,
    size: u64,
  ) -> io::Result<()> {
    tokio::io::AsyncWriteExt::flush(&mut upload.file).await?;
    upload.file.sync_data().await?;

    let Some(path) = upload.path.take() else {
      return Ok(());
    };

    let target = self.path(key);
    if let Some(shard) = target.parent() {
      tokio::fs::create_dir_all(shard).await?;
    }

    tokio::fs::rename(&path, &target).await?;

    let evicted = {
      let mut index = self.lock();
      index.insert(key, size);
      index.shrink(self.budget)
    };
    self.delete(&evicted);
    Ok(())
  }

  /// Stores `bytes` as `key` in one go.
  pub async fn write(&self, key: Key, bytes: &[u8]) -> io::Result<()> {
    let mut upload = self.upload().await?;
    tokio::io::AsyncWriteExt::write_all(&mut upload.file, bytes).await?;
    self.commit(upload, key, bytes.len() as u64).await
  }

  fn delete(&self, keys: &[Key]) {
    for &key in keys {
      if let Err(error) = fs::remove_file(self.path(key))
        && error.kind() != io::ErrorKind::NotFound
      {
        tracing::warn!(%key, dir = %self.dir.display(), "failed to evict: {error}");
      }
    }
  }
}

pub struct Upload {
  path:     Option<PathBuf>,
  pub file: File,
}

impl Drop for Upload {
  fn drop(&mut self) {
    if let Some(path) = self.path.take() {
      drop(fs::remove_file(path));
    }
  }
}

async fn refresh_mtime(file: &File) {
  let Ok(metadata) = file.metadata().await else {
    return;
  };

  let stale = metadata
    .modified()
    .ok()
    .and_then(|modified| SystemTime::now().duration_since(modified).ok())
    .is_none_or(|age| age > TOUCH_INTERVAL);

  if stale && let Ok(std_file) = file.try_clone().await {
    let std_file = std_file.into_std().await;
    drop(tokio::task::spawn_blocking(move || {
      std_file.set_modified(SystemTime::now())
    }));
  }
}
