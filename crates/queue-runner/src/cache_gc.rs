use std::{future::pending, time::Duration};

use circus_common::{PgPool, repo::narinfo_cache};
use circus_config::{CacheGcConfig, CacheUploadConfig};
use jiff::{SignedDuration, Timestamp};

pub async fn run(
  config: CacheGcConfig,
  upload: CacheUploadConfig,
  pool: PgPool,
) {
  if !config.is_enabled() {
    return pending().await;
  }

  let presigner = upload
    .store_uri
    .as_deref()
    .zip(upload.s3.as_ref())
    .and_then(|(uri, s3)| circus_s3::Presigner::from_config(uri, s3));
  let Some(presigner) = presigner else {
    tracing::error!(
      "Cache GC is enabled but the S3 object store cannot be authenticated"
    );
    return pending().await;
  };
  let client = match circus_http::api().build() {
    Ok(client) => client,
    Err(error) => {
      tracing::error!(%error, "Failed to initialize the cache GC HTTP client");
      return pending().await;
    },
  };

  #[expect(
    clippy::infinite_loop,
    reason = "intentional background cleanup loop"
  )]
  loop {
    tokio::time::sleep(Duration::from_secs(config.cleanup_interval)).await;
    if let Err(error) = run_cycle(&config, &presigner, &client, &pool).await {
      tracing::error!(%error, "Automatic cache cleanup failed");
    }
  }
}

async fn run_cycle(
  config: &CacheGcConfig,
  presigner: &circus_s3::Presigner,
  client: &reqwest::Client,
  pool: &PgPool,
) -> circus_common::Result<()> {
  let cutoff = config.max_age_days.map(|days| {
    Timestamp::now() - SignedDuration::from_hours(24 * i64::from(days))
  });
  let candidates = narinfo_cache::list_gc_candidates(
    pool,
    cutoff,
    config.max_size_bytes,
    config.target_size_bytes,
  )
  .await?;
  if candidates.is_empty() {
    return Ok(());
  }

  let deleted = delete_objects(client, presigner, candidates).await;
  let reclaimed = deleted.iter().fold(0_i64, |total, candidate| {
    total.saturating_add(candidate.bytes)
  });
  let store_paths = deleted
    .into_iter()
    .map(|candidate| candidate.store_path)
    .collect::<Vec<_>>();
  let metadata_rows =
    narinfo_cache::delete_gc_candidates(pool, &store_paths).await?;
  tracing::info!(
    objects = store_paths.len(),
    metadata_rows,
    reclaimed_bytes = reclaimed,
    "Automatic cache cleanup completed"
  );
  Ok(())
}

async fn delete_objects(
  client: &reqwest::Client,
  presigner: &circus_s3::Presigner,
  candidates: Vec<narinfo_cache::CacheGcCandidate>,
) -> Vec<narinfo_cache::CacheGcCandidate> {
  let objects = candidates
    .into_iter()
    .map(|candidate| {
      let key = candidate.url.clone();
      (candidate, key)
    })
    .collect();

  presigner
    .delete_all(client, objects)
    .await
    .into_iter()
    .filter_map(|(candidate, result)| {
      let Err(error) = result else {
        return Some(candidate);
      };
      tracing::warn!(
        store_path = %candidate.store_path,
        error = %circus_logs::ErrorChain(&error),
        "Failed to delete cache object"
      );
      None
    })
    .collect()
}
