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
