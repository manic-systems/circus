use axum::{
  Router,
  extract::{Query, State},
  http::StatusCode,
  response::{IntoResponse, Response},
  routing::get,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::state::AppState;

/// Query parameters for timeseries data
#[derive(Debug, Deserialize)]
struct TimeseriesQuery {
  project_id: Option<Uuid>,
  jobset_id:  Option<Uuid>,
  #[serde(default = "default_hours")]
  hours:      i32,
  #[serde(default = "default_bucket")]
  bucket:     i32,
}

const fn default_hours() -> i32 {
  24
}

const fn default_bucket() -> i32 {
  60
}

/// Response type for build stats timeseries
#[derive(serde::Serialize)]
struct BuildStatsResponse {
  timestamps:   Vec<String>,
  total:        Vec<i64>,
  failed:       Vec<i64>,
  avg_duration: Vec<Option<f64>>,
}

/// Response type for duration percentiles
#[derive(serde::Serialize)]
struct DurationPercentilesResponse {
  timestamps: Vec<String>,
  p50:        Vec<Option<f64>>,
  p95:        Vec<Option<f64>>,
  p99:        Vec<Option<f64>>,
}

/// Response type for system distribution
#[derive(serde::Serialize)]
struct SystemDistributionResponse {
  systems: Vec<String>,
  counts:  Vec<i64>,
}

/// Escape a string for use as a Prometheus label value.
/// Per the exposition format, backslash, double-quote, and newline must be
/// escaped.
fn escape_prometheus_label(s: &str) -> String {
  s.replace('\\', "\\\\")
    .replace('"', "\\\"")
    .replace('\n', "\\n")
}

async fn prometheus_metrics(State(state): State<AppState>) -> Response {
  use std::fmt::Write;

  let Ok(build_stats) =
    circus_common::repo::builds::get_stats(&state.pool).await
  else {
    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
  };

  let eval_count: i64 =
    sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM evaluations")
      .fetch_one(&state.pool)
      .await
      .ok()
      .map_or(0, |row| row.0);

  let eval_by_status: Vec<(String, i64)> = sqlx::query_as(
    "SELECT status::text, COUNT(*) FROM evaluations GROUP BY status",
  )
  .fetch_all(&state.pool)
  .await
  .unwrap_or_default();

  let (project_count, channel_count, builder_count): (i64, i64, i64) =
    sqlx::query_as(
      "SELECT (SELECT COUNT(*) FROM projects), (SELECT COUNT(*) FROM \
       channels), (SELECT COUNT(*) FROM remote_builders WHERE enabled = true)",
    )
    .fetch_one(&state.pool)
    .await
    .unwrap_or((0, 0, 0));

  // Per-project build counts
  let per_project: Vec<(String, i64, i64)> = sqlx::query_as(
    "SELECT p.name, COUNT(*) FILTER (WHERE b.status = 'succeeded'), COUNT(*) \
     FILTER (WHERE b.status = 'failed') FROM builds b JOIN evaluations e ON \
     b.evaluation_id = e.id JOIN jobsets j ON e.jobset_id = j.id JOIN \
     projects p ON j.project_id = p.id GROUP BY p.name",
  )
  .fetch_all(&state.pool)
  .await
  .unwrap_or_default();

  // Build duration percentiles (single query)
  let (duration_p50, duration_p95, duration_p99): (
    Option<f64>,
    Option<f64>,
    Option<f64>,
  ) = sqlx::query_as(
    "SELECT (PERCENTILE_CONT(0.5) WITHIN GROUP (ORDER BY EXTRACT(EPOCH FROM \
     (completed_at - started_at)))), (PERCENTILE_CONT(0.95) WITHIN GROUP \
     (ORDER BY EXTRACT(EPOCH FROM (completed_at - started_at)))), \
     (PERCENTILE_CONT(0.99) WITHIN GROUP (ORDER BY EXTRACT(EPOCH FROM \
     (completed_at - started_at)))) FROM builds WHERE completed_at IS NOT \
     NULL AND started_at IS NOT NULL",
  )
  .fetch_one(&state.pool)
  .await
  .unwrap_or((None, None, None));

  let mut output = String::with_capacity(2048);

  // Build counts by status
  output
    .push_str("# HELP circus_builds_total Total number of builds by status\n");
  output.push_str("# TYPE circus_builds_total gauge\n");
  let _ = writeln!(
    output,
    "circus_builds_total{{status=\"succeeded\"}} {}",
    build_stats.completed_builds.unwrap_or(0)
  );
  let _ = writeln!(
    output,
    "circus_builds_total{{status=\"failed\"}} {}",
    build_stats.failed_builds.unwrap_or(0)
  );
  let _ = writeln!(
    output,
    "circus_builds_total{{status=\"running\"}} {}",
    build_stats.running_builds.unwrap_or(0)
  );
  let _ = writeln!(
    output,
    "circus_builds_total{{status=\"pending\"}} {}",
    build_stats.pending_builds.unwrap_or(0)
  );
  let _ = writeln!(
    output,
    "circus_builds_total{{status=\"all\"}} {}",
    build_stats.total_builds.unwrap_or(0)
  );

  // Build duration stats
  output.push_str(
    "\n# HELP circus_builds_avg_duration_seconds Average build duration in \
     seconds\n",
  );
  output.push_str("# TYPE circus_builds_avg_duration_seconds gauge\n");
  let _ = writeln!(
    output,
    "circus_builds_avg_duration_seconds {:.2}",
    build_stats.avg_duration_seconds.unwrap_or(0.0)
  );

  output.push_str(
    "\n# HELP circus_builds_duration_seconds Build duration percentiles\n",
  );
  output.push_str("# TYPE circus_builds_duration_seconds gauge\n");
  if let Some(p50) = duration_p50 {
    let _ = writeln!(
      output,
      "circus_builds_duration_seconds{{quantile=\"0.5\"}} {p50:.2}"
    );
  }
  if let Some(p95) = duration_p95 {
    let _ = writeln!(
      output,
      "circus_builds_duration_seconds{{quantile=\"0.95\"}} {p95:.2}"
    );
  }
  if let Some(p99) = duration_p99 {
    let _ = writeln!(
      output,
      "circus_builds_duration_seconds{{quantile=\"0.99\"}} {p99:.2}"
    );
  }

  // Evaluations
  output.push_str(
    "\n# HELP circus_evaluations_total Total number of evaluations\n",
  );
  output.push_str("# TYPE circus_evaluations_total gauge\n");
  let _ = writeln!(output, "circus_evaluations_total {eval_count}");

  output
    .push_str("\n# HELP circus_evaluations_by_status Evaluations by status\n");
  output.push_str("# TYPE circus_evaluations_by_status gauge\n");
  for (status, count) in &eval_by_status {
    let _ = writeln!(
      output,
      "circus_evaluations_by_status{{status=\"{status}\"}} {count}"
    );
  }

  // Queue depth (pending builds)
  output.push_str(
    "\n# HELP circus_queue_depth Number of pending builds in queue\n",
  );
  output.push_str("# TYPE circus_queue_depth gauge\n");
  let _ = writeln!(
    output,
    "circus_queue_depth {}",
    build_stats.pending_builds.unwrap_or(0)
  );

  // Infrastructure
  output.push_str("\n# HELP circus_projects_total Total number of projects\n");
  output.push_str("# TYPE circus_projects_total gauge\n");
  let _ = writeln!(output, "circus_projects_total {project_count}");

  output.push_str("\n# HELP circus_channels_total Total number of channels\n");
  output.push_str("# TYPE circus_channels_total gauge\n");
  let _ = writeln!(output, "circus_channels_total {channel_count}");

  output.push_str(
    "\n# HELP circus_remote_builders_active Active remote builders\n",
  );
  output.push_str("# TYPE circus_remote_builders_active gauge\n");
  let _ = writeln!(output, "circus_remote_builders_active {builder_count}");

  // Per-project build counts
  if !per_project.is_empty() {
    output.push_str(
      "\n# HELP circus_project_builds_completed Completed builds per project\n",
    );
    output.push_str("# TYPE circus_project_builds_completed gauge\n");
    for (name, completed, _) in &per_project {
      let escaped = escape_prometheus_label(name);
      let _ = writeln!(
        output,
        "circus_project_builds_completed{{project=\"{escaped}\"}} {completed}"
      );
    }
    output.push_str(
      "\n# HELP circus_project_builds_failed Failed builds per project\n",
    );
    output.push_str("# TYPE circus_project_builds_failed gauge\n");
    for (name, _, failed) in &per_project {
      let escaped = escape_prometheus_label(name);
      let _ = writeln!(
        output,
        "circus_project_builds_failed{{project=\"{escaped}\"}} {failed}"
      );
    }
  }

  (
    StatusCode::OK,
    [("content-type", "text/plain; version=0.0.4; charset=utf-8")],
    output,
  )
    .into_response()
}

/// Get build statistics timeseries data for visualization
async fn build_stats_timeseries(
  State(state): State<AppState>,
  Query(params): Query<TimeseriesQuery>,
) -> Response {
  match circus_common::repo::build_metrics::get_build_stats_timeseries(
    &state.pool,
    params.project_id,
    params.jobset_id,
    params.hours,
    params.bucket,
  )
  .await
  {
    Ok(buckets) => {
      let response = BuildStatsResponse {
        timestamps:   buckets
          .iter()
          .map(|b| b.bucket_time.format("%Y-%m-%dT%H:%M:%SZ").to_string())
          .collect(),
        total:        buckets.iter().map(|b| b.total_builds).collect(),
        failed:       buckets.iter().map(|b| b.failed_builds).collect(),
        avg_duration: buckets.iter().map(|b| b.avg_duration).collect(),
      };
      (StatusCode::OK, axum::Json(response)).into_response()
    },
    Err(e) => {
      tracing::error!("Failed to fetch build stats timeseries: {e}");
      StatusCode::INTERNAL_SERVER_ERROR.into_response()
    },
  }
}

/// Get duration percentile timeseries data
async fn duration_percentiles_timeseries(
  State(state): State<AppState>,
  Query(params): Query<TimeseriesQuery>,
) -> Response {
  match circus_common::repo::build_metrics::get_duration_percentiles_timeseries(
    &state.pool,
    params.project_id,
    params.jobset_id,
    params.hours,
    params.bucket,
  )
  .await
  {
    Ok(buckets) => {
      let response = DurationPercentilesResponse {
        timestamps: buckets
          .iter()
          .map(|b| b.bucket_time.format("%Y-%m-%dT%H:%M:%SZ").to_string())
          .collect(),
        p50:        buckets.iter().map(|b| b.p50).collect(),
        p95:        buckets.iter().map(|b| b.p95).collect(),
        p99:        buckets.iter().map(|b| b.p99).collect(),
      };
      (StatusCode::OK, axum::Json(response)).into_response()
    },
    Err(e) => {
      tracing::error!("Failed to fetch duration percentiles: {e}");
      StatusCode::INTERNAL_SERVER_ERROR.into_response()
    },
  }
}

/// Get system distribution data
async fn system_distribution(
  State(state): State<AppState>,
  Query(params): Query<TimeseriesQuery>,
) -> Response {
  match circus_common::repo::build_metrics::get_system_distribution(
    &state.pool,
    params.project_id,
    params.hours,
  )
  .await
  {
    Ok(distribution) => {
      let (systems, counts): (Vec<String>, Vec<i64>) =
        distribution.into_iter().unzip();
      let response = SystemDistributionResponse { systems, counts };
      (StatusCode::OK, axum::Json(response)).into_response()
    },
    Err(e) => {
      tracing::error!("Failed to fetch system distribution: {e}");
      StatusCode::INTERNAL_SERVER_ERROR.into_response()
    },
  }
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/prometheus", get(prometheus_metrics))
    .route(
      "/api/v1/metrics/timeseries/builds",
      get(build_stats_timeseries),
    )
    .route(
      "/api/v1/metrics/timeseries/duration",
      get(duration_percentiles_timeseries),
    )
    .route("/api/v1/metrics/systems", get(system_distribution))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_escape_prometheus_label_plain() {
    assert_eq!(escape_prometheus_label("hello"), "hello");
  }

  #[test]
  fn test_escape_prometheus_label_backslash() {
    assert_eq!(escape_prometheus_label(r"a\b"), r"a\\b");
  }

  #[test]
  fn test_escape_prometheus_label_quotes() {
    assert_eq!(escape_prometheus_label(r#"say "hi""#), r#"say \"hi\""#);
  }

  #[test]
  fn test_escape_prometheus_label_newline() {
    assert_eq!(escape_prometheus_label("line1\nline2"), r"line1\nline2");
  }

  #[test]
  fn test_escape_prometheus_label_combined() {
    assert_eq!(escape_prometheus_label("a\\b\n\"c\""), r#"a\\b\n\"c\""#);
  }
}
