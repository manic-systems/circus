use axum::Json;

pub(super) async fn api_projects() -> Json<serde_json::Value> {
  Json(serde_json::json!({
    "data": [
      { "id": "00000000-0000-0000-0000-000000000001", "name": "circus" }
    ]
  }))
}

pub(super) async fn api_ok() -> Json<serde_json::Value> {
  Json(serde_json::json!({ "ok": true }))
}

pub(super) async fn api_project_create(
  Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
  let name = body
    .get("name")
    .and_then(serde_json::Value::as_str)
    .unwrap_or("circus-preview");

  Json(serde_json::json!({
    "id": "00000000-0000-0000-0000-000000000001",
    "name": name
  }))
}

pub(super) async fn api_project_jobset_create(
  Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
  let name = body
    .get("name")
    .and_then(serde_json::Value::as_str)
    .unwrap_or("preview-jobset");

  Json(serde_json::json!({
    "id": "00000000-0000-0000-0000-000000000011",
    "name": name,
    "enabled": true
  }))
}

pub(super) async fn api_project_probe(
  Json(_body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
  Json(serde_json::json!({
    "is_flake": true,
    "outputs": [
      {
        "path": "packages.x86_64-linux.circus-server",
        "output_type": "derivation",
        "systems": ["x86_64-linux", "aarch64-linux", "aarch64-darwin"]
      },
      {
        "path": "checks.x86_64-linux.clippy",
        "output_type": "derivation",
        "systems": ["x86_64-linux", "aarch64-linux"]
      }
    ],
    "suggested_jobsets": [
      {
        "name": "packages",
        "nix_expression": "packages",
        "description": "Build package outputs",
        "priority": 8,
        "systems": ["x86_64-linux", "aarch64-linux", "aarch64-darwin"]
      },
      {
        "name": "checks",
        "nix_expression": "checks",
        "description": "Run flake checks",
        "priority": 6,
        "systems": ["x86_64-linux", "aarch64-linux"]
      }
    ],
    "metadata": {
      "description": "Fixture flake used by the frontend preview",
      "url": "https://example.invalid/circus-preview"
    },
    "error": null
  }))
}

pub(super) async fn api_project_setup(
  Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
  let name = body
    .get("name")
    .and_then(serde_json::Value::as_str)
    .unwrap_or("circus-preview");

  Json(serde_json::json!({
    "project": {
      "id": "00000000-0000-0000-0000-000000000001",
      "name": name
    },
    "jobsets": body
      .get("jobsets")
      .cloned()
      .unwrap_or_else(|| serde_json::json!([]))
  }))
}

pub(super) async fn api_key_create(
  Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
  let name = body
    .get("name")
    .and_then(serde_json::Value::as_str)
    .unwrap_or("preview-key");
  let role = body
    .get("role")
    .and_then(serde_json::Value::as_str)
    .unwrap_or("read-only");

  Json(serde_json::json!({
    "key": "circus_preview_key_000000",
    "api_key": {
      "id": "00000000-0000-0000-0000-00000000002b",
      "name": name,
      "role": role
    }
  }))
}

pub(super) async fn api_user_create(
  Json(body): Json<serde_json::Value>,
) -> Json<serde_json::Value> {
  let username = body
    .get("username")
    .and_then(serde_json::Value::as_str)
    .unwrap_or("preview-user");

  Json(serde_json::json!({
    "id": "00000000-0000-0000-0000-000000000031",
    "username": username,
    "enabled": true
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
