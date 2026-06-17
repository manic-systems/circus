use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{error::Result, models::BuildMetric};

type PercentileRow = (DateTime<Utc>, Option<f64>, Option<f64>, Option<f64>);

/// Time-series data point for metrics visualization.
#[derive(Debug, Clone)]
pub struct TimeseriesPoint {
  pub timestamp: DateTime<Utc>,
  pub value:     f64,
}

/// Build statistics for a time bucket.
#[derive(Debug, Clone)]
pub struct BuildStatsBucket {
  pub bucket_time:   DateTime<Utc>,
  pub total_builds:  i64,
  pub failed_builds: i64,
  pub avg_duration:  Option<f64>,
}

/// Duration percentile data for a time bucket.
#[derive(Debug, Clone)]
pub struct DurationPercentiles {
  pub bucket_time: DateTime<Utc>,
  pub p50:         Option<f64>,
  pub p95:         Option<f64>,
  pub p99:         Option<f64>,
}

/// Insert or update a build metric.
///
/// # Errors
///
/// Returns error if database operation fails.
pub async fn upsert(
  pool: &PgPool,
  build_id: Uuid,
  metric_name: &str,
  metric_value: f64,
  unit: &str,
) -> Result<BuildMetric> {
  Ok(
    sqlx::query_as::<_, BuildMetric>(
      "INSERT INTO build_metrics (build_id, metric_name, metric_value, unit) \
       VALUES ($1, $2, $3, $4) ON CONFLICT (build_id, metric_name) DO UPDATE \
       SET metric_value = EXCLUDED.metric_value, collected_at = NOW() \
       RETURNING *",
    )
    .bind(build_id)
    .bind(metric_name)
    .bind(metric_value)
    .bind(unit)
    .fetch_one(pool)
    .await?,
  )
}

/// Calculate build failure rate over a time window.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn calculate_failure_rate(
  pool: &PgPool,
  project_id: Option<Uuid>,
  jobset_id: Option<Uuid>,
  window_minutes: i64,
) -> Result<f64> {
  let rows: Vec<(Uuid, String)> = sqlx::query_as(
    "SELECT b.id, b.status::text FROM builds b JOIN evaluations e ON \
     b.evaluation_id = e.id JOIN jobsets j ON e.jobset_id = j.id WHERE \
     ($1::uuid IS NULL OR j.project_id = $1) AND ($2::uuid IS NULL OR j.id = \
     $2) AND b.completed_at > NOW() - (INTERVAL '1 minute' * $3) ORDER BY \
     b.completed_at DESC",
  )
  .bind(project_id)
  .bind(jobset_id)
  .bind(window_minutes)
  .fetch_all(pool)
  .await?;

  if rows.is_empty() {
    return Ok(0.0);
  }

  let failed_count = rows
    .iter()
    .filter(|(_, status)| *status == "Failed")
    .count();
  Ok((failed_count as f64) / (rows.len() as f64) * 100.0)
}

/// Get build success/failure counts over time.
/// Buckets builds by time interval for charting.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get_build_stats_timeseries(
  pool: &PgPool,
  project_id: Option<Uuid>,
  jobset_id: Option<Uuid>,
  hours: i32,
  bucket_minutes: i32,
) -> Result<Vec<BuildStatsBucket>> {
  let rows: Vec<(DateTime<Utc>, i64, i64, Option<f64>)> = sqlx::query_as(
    "SELECT 
      date_trunc('minute', b.completed_at) + 
        (EXTRACT(MINUTE FROM b.completed_at)::int / $4) * INTERVAL '1 minute' \
     * $4 AS bucket_time,
      COUNT(*) AS total_builds,
      COUNT(*) FILTER (WHERE b.status = 'failed') AS failed_builds,
      AVG(EXTRACT(EPOCH FROM (b.completed_at - b.started_at))) AS avg_duration
    FROM builds b
    JOIN evaluations e ON b.evaluation_id = e.id
    JOIN jobsets j ON e.jobset_id = j.id
    WHERE b.completed_at IS NOT NULL
      AND b.completed_at > NOW() - (INTERVAL '1 hour' * $1)
      AND ($2::uuid IS NULL OR j.project_id = $2)
      AND ($3::uuid IS NULL OR j.id = $3)
    GROUP BY bucket_time
    ORDER BY bucket_time ASC",
  )
  .bind(hours)
  .bind(project_id)
  .bind(jobset_id)
  .bind(bucket_minutes)
  .fetch_all(pool)
  .await?;

  Ok(
    rows
      .into_iter()
      .map(|(bucket_time, total_builds, failed_builds, avg_duration)| {
        BuildStatsBucket {
          bucket_time,
          total_builds,
          failed_builds,
          avg_duration,
        }
      })
      .collect(),
  )
}

/// Get build duration percentiles over time.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get_duration_percentiles_timeseries(
  pool: &PgPool,
  project_id: Option<Uuid>,
  jobset_id: Option<Uuid>,
  hours: i32,
  bucket_minutes: i32,
) -> Result<Vec<DurationPercentiles>> {
  let rows: Vec<PercentileRow> = sqlx::query_as(
    "SELECT 
      date_trunc('minute', b.completed_at) + 
        (EXTRACT(MINUTE FROM b.completed_at)::int / $4) * INTERVAL '1 minute' \
     * $4 AS bucket_time,
      PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY EXTRACT(EPOCH FROM \
     (b.completed_at - b.started_at))) AS p50,
      PERCENTILE_CONT(0.95) WITHIN GROUP (ORDER BY EXTRACT(EPOCH FROM \
     (b.completed_at - b.started_at))) AS p95,
      PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY EXTRACT(EPOCH FROM \
     (b.completed_at - b.started_at))) AS p99
    FROM builds b
    JOIN evaluations e ON b.evaluation_id = e.id
    JOIN jobsets j ON e.jobset_id = j.id
    WHERE b.completed_at IS NOT NULL
      AND b.started_at IS NOT NULL
      AND b.completed_at > NOW() - (INTERVAL '1 hour' * $1)
      AND ($2::uuid IS NULL OR j.project_id = $2)
      AND ($3::uuid IS NULL OR j.id = $3)
    GROUP BY bucket_time
    ORDER BY bucket_time ASC",
  )
  .bind(hours)
  .bind(project_id)
  .bind(jobset_id)
  .bind(bucket_minutes)
  .fetch_all(pool)
  .await?;

  Ok(
    rows
      .into_iter()
      .map(|(bucket_time, p50, p95, p99)| {
        DurationPercentiles {
          bucket_time,
          p50,
          p95,
          p99,
        }
      })
      .collect(),
  )
}

/// Get queue depth over time.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get_queue_depth_timeseries(
  pool: &PgPool,
  hours: i32,
  bucket_minutes: i32,
) -> Result<Vec<TimeseriesPoint>> {
  // Since we don't have historical queue depth, we'll sample current pending
  // builds and use build creation times to approximate queue depth over time
  let rows: Vec<(DateTime<Utc>, i64)> = sqlx::query_as(
    "SELECT 
      date_trunc('minute', created_at) + 
        (EXTRACT(MINUTE FROM created_at)::int / $2) * INTERVAL '1 minute' * $2 \
     AS bucket_time,
      COUNT(*) FILTER (WHERE status = 'pending') AS pending_count
    FROM builds
    WHERE created_at > NOW() - (INTERVAL '1 hour' * $1)
    GROUP BY bucket_time
    ORDER BY bucket_time ASC",
  )
  .bind(hours)
  .bind(bucket_minutes)
  .fetch_all(pool)
  .await?;

  Ok(
    rows
      .into_iter()
      .map(|(timestamp, value)| {
        TimeseriesPoint {
          timestamp,
          value: value as f64,
        }
      })
      .collect(),
  )
}

/// Get per-system build distribution.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get_system_distribution(
  pool: &PgPool,
  project_id: Option<Uuid>,
  hours: i32,
) -> Result<Vec<(String, i64)>> {
  Ok(
    sqlx::query_as(
      "SELECT 
      COALESCE(b.system, 'unknown') AS system,
      COUNT(*) AS build_count
    FROM builds b
    JOIN evaluations e ON b.evaluation_id = e.id
    JOIN jobsets j ON e.jobset_id = j.id
    WHERE b.completed_at > NOW() - (INTERVAL '1 hour' * $1)
      AND ($2::uuid IS NULL OR j.project_id = $2)
    GROUP BY b.system
    ORDER BY build_count DESC",
    )
    .bind(hours)
    .bind(project_id)
    .fetch_all(pool)
    .await?,
  )
}
