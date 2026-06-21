use axum::Json;

pub(super) async fn api_projects() -> Json<serde_json::Value> {
  Json(serde_json::json!({
    "data": [
      { "id": "00000000-0000-0000-0000-000000000001", "name": "circus" }
    ]
  }))
}

pub(super) async fn api_metrics_builds() -> Json<serde_json::Value> {
  Json(serde_json::json!({
    "timestamps": [
      "2026-06-18T08:00:00Z",
      "2026-06-18T09:00:00Z",
      "2026-06-18T10:00:00Z",
      "2026-06-18T11:00:00Z",
      "2026-06-18T12:00:00Z"
    ],
    "total": [12, 18, 14, 22, 16],
    "failed": [1, 2, 0, 3, 1]
  }))
}

pub(super) async fn api_metrics_duration() -> Json<serde_json::Value> {
  Json(serde_json::json!({
    "timestamps": [
      "2026-06-18T08:00:00Z",
      "2026-06-18T09:00:00Z",
      "2026-06-18T10:00:00Z",
      "2026-06-18T11:00:00Z",
      "2026-06-18T12:00:00Z"
    ],
    "p50": [45, 52, 48, 61, 55],
    "p95": [180, 210, 195, 240, 220],
    "p99": [300, 340, 310, 380, 350]
  }))
}

pub(super) async fn api_metrics_systems() -> Json<serde_json::Value> {
  Json(serde_json::json!({
    "systems": ["x86_64-linux", "aarch64-linux"],
    "counts": [42, 18]
  }))
}

pub(super) async fn api_cache_storage_timeseries() -> Json<serde_json::Value> {
  Json(serde_json::json!({
    "timestamps": [
      "2026-06-16T08:00:00Z",
      "2026-06-17T08:00:00Z",
      "2026-06-18T08:00:00Z"
    ],
    "bytes_added": [3_500_000, 7_200_000, 10_800_000],
    "packages_added": [8, 12, 10]
  }))
}

pub(super) async fn api_cache_traffic_timeseries() -> Json<serde_json::Value> {
  Json(serde_json::json!({
    "timestamps": [
      "2026-06-18T08:00:00Z",
      "2026-06-18T09:00:00Z",
      "2026-06-18T10:00:00Z",
      "2026-06-18T11:00:00Z",
      "2026-06-18T12:00:00Z"
    ],
    "bytes": [1_200_000, 2_800_000, 1_500_000, 3_100_000, 900_000],
    "requests": [42, 95, 53, 108, 31]
  }))
}
