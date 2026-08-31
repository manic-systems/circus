use chrono::{DateTime, Utc};
use circus_codegen::queries::build_metrics as q;
use rust_decimal::prelude::ToPrimitive;
use uuid::Uuid;

use crate::{db::PgPool, error::Result, models::BuildMetric};

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

/// Evaluation count grouped by status.
#[derive(Debug, Clone)]
pub struct EvaluationStatusCount {
  pub status: String,
  pub count:  i64,
}

/// Small global counters used by Prometheus exposition.
#[derive(Debug, Clone, Copy, Default)]
pub struct OverviewCounts {
  pub project_count: i64,
  pub channel_count: i64,
}

/// Build success/failure counters grouped by project name.
#[derive(Debug, Clone)]
pub struct ProjectBuildCounts {
  pub name:            String,
  pub succeeded_count: i64,
  pub failed_count:    i64,
}

/// Overall build duration percentiles.
#[derive(Debug, Clone, Copy, Default)]
pub struct OverallDurationPercentiles {
  pub p50: Option<f64>,
  pub p95: Option<f64>,
  pub p99: Option<f64>,
}

impl From<q::BuildMetricRow> for BuildMetric {
  fn from(r: q::BuildMetricRow) -> Self {
    Self {
      id:           r.id,
      build_id:     r.build_id,
      metric_name:  r.metric_name,
      metric_value: r.metric_value,
      unit:         r.unit,
      collected_at: r.collected_at,
    }
  }
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
  let client = pool.get().await?;
  Ok(
    q::upsert()
      .bind(&client, &build_id, &metric_name, &metric_value, &unit)
      .one()
      .await
      .map(BuildMetric::from)?,
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
  let client = pool.get().await?;
  let window_minutes = window_minutes as f64;
  let rows = q::calculate_failure_rate()
    .bind(&client, &project_id, &jobset_id, &window_minutes)
    .all()
    .await?;

  if rows.is_empty() {
    return Ok(0.0);
  }

  let failed_count = rows.iter().filter(|r| r.status == "Failed").count();
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
  let client = pool.get().await?;
  let hours = f64::from(hours);
  let rows = q::get_build_stats_timeseries()
    .bind(&client, &bucket_minutes, &hours, &project_id, &jobset_id)
    .all()
    .await?;

  Ok(
    rows
      .into_iter()
      .map(|r| {
        BuildStatsBucket {
          bucket_time:   r.bucket_time,
          total_builds:  r.total_builds,
          failed_builds: r.failed_builds,
          avg_duration:  r.avg_duration.and_then(|d| d.to_f64()),
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
  let client = pool.get().await?;
  let hours = f64::from(hours);
  let rows = q::get_duration_percentiles_timeseries()
    .bind(&client, &bucket_minutes, &hours, &project_id, &jobset_id)
    .all()
    .await?;

  Ok(
    rows
      .into_iter()
      .map(|r| {
        DurationPercentiles {
          bucket_time: r.bucket_time,
          p50:         r.p50,
          p95:         r.p95,
          p99:         r.p99,
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
  let client = pool.get().await?;
  let hours = f64::from(hours);
  let rows = q::get_queue_depth_timeseries()
    .bind(&client, &bucket_minutes, &hours)
    .all()
    .await?;

  Ok(
    rows
      .into_iter()
      .map(|r| {
        TimeseriesPoint {
          timestamp: r.bucket_time,
          value:     r.pending_count as f64,
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
  let client = pool.get().await?;
  let hours = f64::from(hours);
  let rows = q::get_system_distribution()
    .bind(&client, &hours, &project_id)
    .all()
    .await?;

  Ok(
    rows
      .into_iter()
      .map(|r| (r.system, r.build_count))
      .collect(),
  )
}

/// Count all evaluations.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count_evaluations(pool: &PgPool) -> Result<i64> {
  let client = pool.get().await?;
  Ok(q::count_evaluations().bind(&client).one().await?)
}

/// Count evaluations grouped by status.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn evaluations_by_status(
  pool: &PgPool,
) -> Result<Vec<EvaluationStatusCount>> {
  let client = pool.get().await?;
  let rows = q::evaluations_by_status().bind(&client).all().await?;
  Ok(
    rows
      .into_iter()
      .map(|r| {
        EvaluationStatusCount {
          status: r.status,
          count:  r.count,
        }
      })
      .collect(),
  )
}

/// Global entity counters for metrics output.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn overview_counts(pool: &PgPool) -> Result<OverviewCounts> {
  let client = pool.get().await?;
  let row = q::overview_counts().bind(&client).one().await?;
  Ok(OverviewCounts {
    project_count: row.project_count,
    channel_count: row.channel_count,
  })
}

/// Count succeeded and failed builds grouped by project.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn per_project_build_counts(
  pool: &PgPool,
) -> Result<Vec<ProjectBuildCounts>> {
  let client = pool.get().await?;
  let rows = q::per_project_build_counts().bind(&client).all().await?;
  Ok(
    rows
      .into_iter()
      .map(|r| {
        ProjectBuildCounts {
          name:            r.name,
          succeeded_count: r.succeeded_count,
          failed_count:    r.failed_count,
        }
      })
      .collect(),
  )
}

/// Overall build duration percentiles.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn duration_percentiles_overall(
  pool: &PgPool,
) -> Result<OverallDurationPercentiles> {
  let client = pool.get().await?;
  let row = q::duration_percentiles_overall()
    .bind(&client)
    .one()
    .await?;
  Ok(OverallDurationPercentiles {
    p50: row.duration_p50,
    p95: row.duration_p95,
    p99: row.duration_p99,
  })
}
