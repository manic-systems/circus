//! Read/write of the `cache_traffic` table plus storage time-series derived
//! from uploaded NAR metadata and signed local build products.
//!
//! The server keeps in-memory per-cache serving counters and a background
//! worker drains them into `cache_traffic` once a minute (see
//! `AppState::spawn_cache_traffic_flush`). The Caches dashboard reads back
//! aggregated traffic and storage series for charting. A cache is identified
//! by name (`global` or a project name), matching how narinfo rows are scoped
//! by `project_id`.

use chrono::{DateTime, Utc};
use circus_codegen::queries::cache_traffic as q;
use serde::Serialize;
use uuid::Uuid;

use crate::{db::PgPool, error::Result};

/// Time-bucket granularity for the cache charts. Each variant maps to a fixed
/// bucket width; the dashboard's Minutes/Hours/Days/Weeks toggle selects one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Granularity {
  Minutes,
  Hours,
  Days,
  Weeks,
}

impl Granularity {
  /// Width of one bucket in seconds.
  const fn bucket_seconds(self) -> i64 {
    match self {
      Self::Minutes => 60,
      Self::Hours => 3_600,
      Self::Days => 86_400,
      Self::Weeks => 604_800,
    }
  }

  /// Parse the dashboard query parameter, defaulting to hourly buckets.
  #[must_use]
  pub fn from_param(s: &str) -> Self {
    match s {
      "minutes" => Self::Minutes,
      "days" => Self::Days,
      "weeks" => Self::Weeks,
      _ => Self::Hours,
    }
  }
}

/// One point in a serving-traffic series.
#[derive(Debug, Clone, Serialize)]
pub struct TrafficPoint {
  pub bucket_time: DateTime<Utc>,
  pub requests:    i64,
  pub bytes:       i64,
}

impl From<q::TrafficTimeseries> for TrafficPoint {
  fn from(r: q::TrafficTimeseries) -> Self {
    Self {
      bucket_time: r.bucket_time,
      requests:    r.requests,
      bytes:       r.bytes,
    }
  }
}

/// One point in a storage-added series (derived from upload timestamps).
#[derive(Debug, Clone, Serialize)]
pub struct StoragePoint {
  pub bucket_time:    DateTime<Utc>,
  pub packages_added: i64,
  pub bytes_added:    i64,
}

impl From<q::StorageTimeseries> for StoragePoint {
  fn from(r: q::StorageTimeseries) -> Self {
    Self {
      bucket_time:    r.bucket_time,
      packages_added: r.packages_added,
      bytes_added:    r.bytes_added,
    }
  }
}

/// Insert one accumulated counter row per cache for this flush tick. A no-op
/// when there is nothing to record.
///
/// # Errors
///
/// Returns the underlying database error.
pub async fn flush(pool: &PgPool, rows: &[(String, i64, i64)]) -> Result<()> {
  if rows.is_empty() {
    return Ok(());
  }
  let names: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
  let requests: Vec<i64> = rows.iter().map(|r| r.1).collect();
  let bytes: Vec<i64> = rows.iter().map(|r| r.2).collect();
  let client = pool.get().await?;
  q::flush().bind(&client, &names, &requests, &bytes).await?;
  Ok(())
}

/// Serving traffic bucketed over the trailing `points * bucket` window for one
/// cache.
///
/// # Errors
///
/// Returns the underlying database error.
pub async fn traffic_timeseries(
  pool: &PgPool,
  cache_name: &str,
  granularity: Granularity,
  points: i64,
) -> Result<Vec<TrafficPoint>> {
  let bucket = granularity.bucket_seconds();
  let window = bucket * points.max(1);
  let client = pool.get().await?;
  let rows = q::traffic_timeseries()
    .bind(&client, &bucket, &cache_name, &window)
    .all()
    .await?;
  Ok(rows.into_iter().map(TrafficPoint::from).collect())
}

/// Storage added over time: uploaded NAR rows plus signed local build products.
/// `project_id = None` is the global cache.
///
/// # Errors
///
/// Returns the underlying database error.
pub async fn storage_timeseries(
  pool: &PgPool,
  project_id: Option<Uuid>,
  granularity: Granularity,
  points: i64,
) -> Result<Vec<StoragePoint>> {
  let bucket = granularity.bucket_seconds();
  let window = bucket * points.max(1);
  let client = pool.get().await?;
  let rows = q::storage_timeseries()
    .bind(&client, &project_id, &bucket, &window)
    .all()
    .await?;
  Ok(rows.into_iter().map(StoragePoint::from).collect())
}

/// Total requests and bytes served for one cache in the trailing hour.
///
/// # Errors
///
/// Returns the underlying database error.
pub async fn traffic_last_hour(
  pool: &PgPool,
  cache_name: &str,
) -> Result<(i64, i64)> {
  let client = pool.get().await?;
  let row = q::traffic_last_hour()
    .bind(&client, &cache_name)
    .one()
    .await?;
  Ok((row.requests, row.bytes_served))
}
