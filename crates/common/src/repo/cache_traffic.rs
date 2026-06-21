//! Read/write of the `cache_traffic` table plus storage time-series derived
//! from `narinfo_cache`.
//!
//! The server keeps in-memory per-cache serving counters and a background
//! worker drains them into `cache_traffic` once a minute (see
//! `AppState::spawn_cache_traffic_flush`). The Caches dashboard reads back
//! aggregated traffic and storage series for charting. A cache is identified
//! by name (`global` or a project name), matching how narinfo rows are scoped
//! by `project_id`.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::Result;

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

/// One point in a storage-added series (derived from upload timestamps).
#[derive(Debug, Clone, Serialize)]
pub struct StoragePoint {
  pub bucket_time:    DateTime<Utc>,
  pub packages_added: i64,
  pub bytes_added:    i64,
}

/// Insert one accumulated counter row per cache for this flush tick. A no-op
/// when there is nothing to record.
///
/// # Errors
///
/// Returns the underlying sqlx error.
pub async fn flush(pool: &PgPool, rows: &[(String, i64, i64)]) -> Result<()> {
  if rows.is_empty() {
    return Ok(());
  }
  let names: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
  let requests: Vec<i64> = rows.iter().map(|r| r.1).collect();
  let bytes: Vec<i64> = rows.iter().map(|r| r.2).collect();
  sqlx::query(
    "INSERT INTO cache_traffic (cache_name, requests, bytes_served) SELECT * \
     FROM UNNEST($1::text[], $2::bigint[], $3::bigint[])",
  )
  .bind(&names)
  .bind(&requests)
  .bind(&bytes)
  .execute(pool)
  .await?;
  Ok(())
}

/// Serving traffic bucketed over the trailing `points * bucket` window for one
/// cache.
///
/// # Errors
///
/// Returns the underlying sqlx error.
pub async fn traffic_timeseries(
  pool: &PgPool,
  cache_name: &str,
  granularity: Granularity,
  points: i64,
) -> Result<Vec<TrafficPoint>> {
  let bucket = granularity.bucket_seconds();
  let window = bucket * points.max(1);
  let rows = sqlx::query_as::<_, (DateTime<Utc>, i64, i64)>(
    "SELECT to_timestamp(floor(extract(epoch FROM recorded_at) / $2) * $2) AS \
     bucket_time, COALESCE(SUM(requests), 0)::bigint, \
     COALESCE(SUM(bytes_served), 0)::bigint FROM cache_traffic WHERE \
     cache_name = $1 AND recorded_at > NOW() - ($3 * INTERVAL '1 second') \
     GROUP BY bucket_time ORDER BY bucket_time ASC",
  )
  .bind(cache_name)
  .bind(bucket)
  .bind(window)
  .fetch_all(pool)
  .await?;
  Ok(
    rows
      .into_iter()
      .map(|(bucket_time, requests, bytes)| {
        TrafficPoint {
          bucket_time,
          requests,
          bytes,
        }
      })
      .collect(),
  )
}

/// Storage added over time, derived from `narinfo_cache.created_at`: packages
/// and on-disk bytes added per bucket for one cache scope. `project_id = None`
/// is the global cache.
///
/// # Errors
///
/// Returns the underlying sqlx error.
pub async fn storage_timeseries(
  pool: &PgPool,
  project_id: Option<Uuid>,
  granularity: Granularity,
  points: i64,
) -> Result<Vec<StoragePoint>> {
  let bucket = granularity.bucket_seconds();
  let window = bucket * points.max(1);
  let rows = sqlx::query_as::<_, (DateTime<Utc>, i64, i64)>(
    "SELECT to_timestamp(floor(extract(epoch FROM created_at) / $2) * $2) AS \
     bucket_time, COUNT(*), COALESCE(SUM(file_size), 0)::bigint FROM \
     narinfo_cache WHERE ($1::uuid IS NULL OR project_id = $1) AND created_at \
     > NOW() - ($3 * INTERVAL '1 second') GROUP BY bucket_time ORDER BY \
     bucket_time ASC",
  )
  .bind(project_id)
  .bind(bucket)
  .bind(window)
  .fetch_all(pool)
  .await?;
  Ok(
    rows
      .into_iter()
      .map(|(bucket_time, packages_added, bytes_added)| {
        StoragePoint {
          bucket_time,
          packages_added,
          bytes_added,
        }
      })
      .collect(),
  )
}

/// Total requests and bytes served for one cache in the trailing hour.
///
/// # Errors
///
/// Returns the underlying sqlx error.
pub async fn traffic_last_hour(
  pool: &PgPool,
  cache_name: &str,
) -> Result<(i64, i64)> {
  let row = sqlx::query_as::<_, (i64, i64)>(
    "SELECT COALESCE(SUM(requests), 0)::bigint, COALESCE(SUM(bytes_served), \
     0)::bigint FROM cache_traffic WHERE cache_name = $1 AND recorded_at > \
     NOW() - INTERVAL '1 hour'",
  )
  .bind(cache_name)
  .fetch_one(pool)
  .await?;
  Ok(row)
}
