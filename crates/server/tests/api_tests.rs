//! Integration tests for API endpoints.
//! Requires `TEST_DATABASE_URL` to be set.
#![expect(clippy::unwrap_used, clippy::print_stdout, reason = "Fine in tests")]

use axum::{
  body::Body,
  http::{Request, StatusCode},
};
use circus_common::models::BinaryCacheUpstreams;
use tower::ServiceExt;

const ADMIN_TOKEN: &str = "circus_test_admin";
const READ_TOKEN: &str = "circus_test_read";

async fn get_pool() -> Option<circus_common::PgPool> {
  static CRYPTO: std::sync::Once = std::sync::Once::new();
  CRYPTO.call_once(|| {
    circus_common::install_crypto_provider()
      .expect("install ring crypto provider");
  });

  let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
    println!("Skipping API test: TEST_DATABASE_URL not set");
    return None;
  };

  // Each test process gets its own database so suite runs cannot pollute
  // data-sensitive tests. nextest runs one process per test.
  let url = per_process_database_url(&url)?;
  circus_migrations::run_migrations(&url)
    .await
    .expect("test database migration failed");

  circus_common::db::build_pool(&url, 5).ok()
}

fn per_process_database_url(url: &str) -> Option<String> {
  let mut parsed = url::Url::parse(url).ok()?;
  let dbname = parsed.path().trim_start_matches('/').to_owned();
  parsed.set_path(&format!("/{dbname}_p{}", std::process::id()));
  Some(parsed.to_string())
}

fn build_app(pool: circus_common::PgPool) -> axum::Router {
  let config = circus_config::Config::default();
  let state = circus_server::state::AppState {
    pool,
    nix_store: circus_server::state::NixStore::new(
      config.nix.store_dir.clone(),
    )
    .unwrap(),
    config: config.clone(),
    sessions: std::sync::Arc::new(dashmap::DashMap::new()),
    narinfo_cache: circus_server::state::AppState::new_narinfo_cache(),
    http_client: reqwest::Client::new(),
    csrf_secret: std::sync::Arc::new([0u8; 32]),
    email_regex: None,
    cache_traffic: std::sync::Arc::new(dashmap::DashMap::new()),
    cache_public_key: None,
  };
  circus_server::routes::router(state, &config)
}

#[tokio::test]
async fn test_router_no_duplicate_routes() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let config = circus_config::Config::default();
  let state = circus_server::state::AppState {
    pool,
    nix_store: circus_server::state::NixStore::new(
      config.nix.store_dir.clone(),
    )
    .unwrap(),
    config: config.clone(),
    sessions: std::sync::Arc::new(dashmap::DashMap::new()),
    narinfo_cache: circus_server::state::AppState::new_narinfo_cache(),
    http_client: reqwest::Client::new(),
    csrf_secret: std::sync::Arc::new([0u8; 32]),
    email_regex: None,
    cache_traffic: std::sync::Arc::new(dashmap::DashMap::new()),
    cache_public_key: None,
  };

  let _app = circus_server::routes::router(state, &config);
}

fn build_app_with_config(
  pool: circus_common::PgPool,
  config: &circus_config::Config,
) -> axum::Router {
  let state = circus_server::state::AppState {
    pool,
    nix_store: circus_server::state::NixStore::new(
      config.nix.store_dir.clone(),
    )
    .unwrap(),
    config: config.clone(),
    sessions: std::sync::Arc::new(dashmap::DashMap::new()),
    narinfo_cache: circus_server::state::AppState::new_narinfo_cache(),
    http_client: reqwest::Client::new(),
    csrf_secret: std::sync::Arc::new([0u8; 32]),
    email_regex: None,
    cache_traffic: std::sync::Arc::new(dashmap::DashMap::new()),
    cache_public_key: None,
  };
  circus_server::routes::router(state, config)
}

fn build_app_public_reads(pool: circus_common::PgPool) -> axum::Router {
  let mut config = circus_config::Config::default();
  config.server.require_api_key_for_reads = false;
  build_app_with_config(pool, &config)
}

async fn ensure_api_key(
  pool: &circus_common::PgPool,
  token: &str,
  role: circus_common::roles::GlobalRole,
) {
  use sha2::Digest;

  let mut hasher = sha2::Sha256::new();
  hasher.update(token.as_bytes());
  let key_hash = hex::encode(hasher.finalize());
  let _ =
    circus_common::repo::api_keys::upsert(pool, token, &key_hash, role).await;
}

async fn build_app_with_admin_key(pool: circus_common::PgPool) -> axum::Router {
  ensure_api_key(&pool, ADMIN_TOKEN, circus_common::roles::GlobalRole::Admin)
    .await;
  build_app(pool)
}

/// A unique 32-char Nix base32 hash for cache tests. A raw uuid hex string is
/// only occasionally valid Nix base32 (the alphabet excludes `e`), which made
/// the cache serve tests flaky, so map that one invalid hex char.
fn unique_nix_hash() -> String {
  uuid::Uuid::new_v4().simple().to_string().replace('e', "z")
}

async fn create_test_project(pool: &circus_common::PgPool) -> uuid::Uuid {
  circus_common::repo::projects::create(pool, circus_common::CreateProject {
    name:            format!("security-test-{}", uuid::Uuid::new_v4()),
    repository_url:  "https://github.com/test/repo".to_string(),
    cache_enabled:   true,
    cache_url:       None,
    cache_upstreams: BinaryCacheUpstreams::default(),
    description:     None,
  })
  .await
  .unwrap()
  .id
}

// API endpoint tests

#[tokio::test]
async fn test_health_endpoint() {
  let Some(pool) = get_pool().await else {
    return;
  };

  for service in [
    circus_common::service_heartbeat::SERVICE_EVALUATOR,
    circus_common::service_heartbeat::SERVICE_QUEUE_RUNNER,
  ] {
    circus_common::service_heartbeat::record(&pool, service, 60, None)
      .await
      .expect("seed heartbeat");
  }

  let app = build_app(pool);

  let response = app
    .oneshot(
      Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);

  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
  assert_eq!(json["status"], "ok");
  assert_eq!(json["database"], true);
}

#[tokio::test]
async fn test_headless_mode_keeps_api_and_health_but_disables_ui() {
  let Some(pool) = get_pool().await else {
    return;
  };

  for service in [
    circus_common::service_heartbeat::SERVICE_EVALUATOR,
    circus_common::service_heartbeat::SERVICE_QUEUE_RUNNER,
  ] {
    circus_common::service_heartbeat::record(&pool, service, 60, None)
      .await
      .expect("seed heartbeat");
  }

  let mut config = circus_config::Config::default();
  config.ui.enabled = false;
  config.server.require_api_key_for_reads = false;
  let app = build_app_with_config(pool, &config);

  let health = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(health.status(), StatusCode::OK);

  let overview = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/api/v1/operator/overview")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(overview.status(), StatusCode::OK);

  let root = app
    .clone()
    .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
    .await
    .unwrap();
  assert_eq!(root.status(), StatusCode::NOT_FOUND);

  let css = app
    .oneshot(
      Request::builder()
        .uri("/static/style.css")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(css.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_full_mode_serves_ui_and_static_assets() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app(pool);
  let root = app
    .clone()
    .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
    .await
    .unwrap();
  assert_eq!(root.status(), StatusCode::OK);

  let css = app
    .oneshot(
      Request::builder()
        .uri("/static/style.css")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(css.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_ui_dashboard_can_be_disabled_independently() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let mut config = circus_config::Config::default();
  config.ui.dashboard = false;
  let app = build_app_with_config(pool, &config);

  let root = app
    .clone()
    .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
    .await
    .unwrap();
  assert_eq!(root.status(), StatusCode::NOT_FOUND);

  let css = app
    .oneshot(
      Request::builder()
        .uri("/static/style.css")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(css.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_ui_assets_can_be_disabled_independently() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let mut config = circus_config::Config::default();
  config.ui.assets = false;
  let app = build_app_with_config(pool, &config);

  let root = app
    .clone()
    .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
    .await
    .unwrap();
  assert_eq!(root.status(), StatusCode::OK);

  let css = app
    .oneshot(
      Request::builder()
        .uri("/static/style.css")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(css.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_ui_theme_css_serves_configured_variables() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let mut config = circus_config::Config::default();
  config
    .ui
    .css_variables
    .insert("accent".to_string(), "#2563eb".to_string());
  let app = build_app_with_config(pool, &config);

  let response = app
    .oneshot(
      Request::builder()
        .uri("/static/theme.css")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let css = String::from_utf8(body.to_vec()).unwrap();
  assert!(css.contains("--accent: #2563eb;"));
}

#[tokio::test]
async fn test_ui_custom_css_file_is_served() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let css_path = std::env::temp_dir()
    .join(format!("circus-custom-{}.css", uuid::Uuid::new_v4()));
  std::fs::write(&css_path, ":root { --accent: #be123c; }\n").unwrap();

  let mut config = circus_config::Config::default();
  config.ui.custom_css = Some(css_path.clone());
  let app = build_app_with_config(pool, &config);

  let response = app
    .oneshot(
      Request::builder()
        .uri("/static/custom.css")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let css = String::from_utf8(body.to_vec()).unwrap();
  assert!(css.contains("--accent: #be123c"));
  let _ = std::fs::remove_file(css_path);
}

#[tokio::test]
async fn test_ui_custom_static_directory_is_served() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let static_dir = std::env::temp_dir()
    .join(format!("circus-static-{}", uuid::Uuid::new_v4()));
  std::fs::create_dir(&static_dir).unwrap();
  std::fs::write(static_dir.join("logo.svg"), "<svg></svg>\n").unwrap();

  let mut config = circus_config::Config::default();
  config.ui.static_dir = Some(static_dir.clone());
  let app = build_app_with_config(pool, &config);

  let response = app
    .oneshot(
      Request::builder()
        .uri("/static/custom/logo.svg")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let svg = String::from_utf8(body.to_vec()).unwrap();
  assert!(svg.contains("<svg>"));
  let _ = std::fs::remove_dir_all(static_dir);
}

#[tokio::test]
async fn test_ui_branding_renders_in_dashboard_shell() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let mut config = circus_config::Config::default();
  config.ui.brand_name = "Acme CI".to_string();
  config.ui.brand_subtitle = "Nix build farm".to_string();
  config.ui.logo_url = Some("/static/custom/logo.svg".to_string());
  config.ui.favicon_url = Some("/static/custom/favicon.svg".to_string());
  config.ui.custom_css = Some(std::env::temp_dir().join("circus-brand.css"));
  let app = build_app_with_config(pool, &config);

  let response = app
    .oneshot(
      Request::builder()
        .uri("/login")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let html = String::from_utf8(body.to_vec()).unwrap();
  assert!(html.contains("Acme CI"));
  assert!(html.contains("Nix build farm"));
  assert!(html.contains("/static/custom/logo.svg"));
  assert!(html.contains("/static/custom/favicon.svg"));
  assert!(html.contains("/static/theme.css"));
  assert!(html.contains("/static/custom.css"));
}

#[tokio::test]
async fn test_project_endpoints() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app_with_admin_key(pool).await;

  // Create project
  let create_body = serde_json::json!({
      "name": format!("api-test-{}", uuid::Uuid::new_v4()),
      "repository_url": "https://github.com/test/repo",
      "description": "Test project"
  });

  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/api/v1/projects")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::from(serde_json::to_vec(&create_body).unwrap()))
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);

  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let project: serde_json::Value = serde_json::from_slice(&body).unwrap();
  let project_id = project["id"].as_str().unwrap();

  // Get project
  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri(format!("/api/v1/projects/{project_id}"))
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);

  // List projects
  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/api/v1/projects")
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);

  // Get non-existent project -> 404
  let fake_id = uuid::Uuid::new_v4();
  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri(format!("/api/v1/projects/{fake_id}"))
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::NOT_FOUND);

  // Delete project
  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/projects/{project_id}"))
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_builds_endpoints() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app_public_reads(pool);

  // Stats endpoint
  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/api/v1/builds/stats")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);

  // Recent endpoint
  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/api/v1/builds/recent")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
}

// Error response structure

#[tokio::test]
async fn test_error_response_includes_error_code() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app_with_admin_key(pool).await;
  let fake_id = uuid::Uuid::new_v4();

  let response = app
    .oneshot(
      Request::builder()
        .uri(format!("/api/v1/projects/{fake_id}"))
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::NOT_FOUND);

  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

  assert_eq!(json["error_code"], "NOT_FOUND");
  assert!(json["error"].as_str().is_some());
}

#[tokio::test]
async fn test_cache_invalid_hash_returns_404() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let mut config = circus_config::Config::default();
  config.cache.enabled = true;
  let app = build_app_with_config(pool, &config);

  // Too short
  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/nix-cache/tooshort.narinfo")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);

  // Contains uppercase
  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/nix-cache/ABCDEFGHIJKLMNOPQRSTUVWXYZABCDEF.narinfo")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);

  // Contains special chars
  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/nix-cache/abcdefghijklmnop!@#$%^&*()abcde.narinfo")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);

  // SQL injection attempt
  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/nix-cache/'%20OR%201=1;%20DROP%20TABLE%20builds;--.narinfo")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);

  // Valid hash format but no matching product -> 404 (not error)
  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/nix-cache/abcdefghijklmnopqrstuvwxyz012345.narinfo")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_cache_serves_only_signed_persisted_narinfo() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let hash = unique_nix_hash();
  let store_path = format!("/nix/store/{hash}-cache-test");
  circus_common::repo::narinfo_cache::upsert(
    &pool,
    circus_common::repo::narinfo_cache::UpsertNarInfo {
      store_path:  &store_path,
      nar_hash:
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      nar_size:    1,
      file_hash:   None,
      file_size:   None,
      compression: "none",
      url:         &format!("nar/{hash}.nar"),
      deriver:     None,
      references:  &[],
      sig:         None,
      ca:          None,
      build_id:    None,
      project_id:  None,
    },
  )
  .await
  .unwrap();

  let mut config = circus_config::Config::default();
  config.cache.enabled = true;
  let app = build_app_with_config(pool.clone(), &config);

  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri(format!("/nix-cache/{hash}.narinfo"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);

  circus_common::repo::narinfo_cache::upsert(
    &pool,
    circus_common::repo::narinfo_cache::UpsertNarInfo {
      store_path:  &store_path,
      nar_hash:
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      nar_size:    1,
      file_hash:   None,
      file_size:   None,
      compression: "none",
      url:         &format!("nar/{hash}.nar"),
      deriver:     None,
      references:  &[],
      sig:         Some("circus:test-signature"),
      ca:          None,
      build_id:    None,
      project_id:  None,
    },
  )
  .await
  .unwrap();

  let app = build_app_with_config(pool, &config);
  let response = app
    .oneshot(
      Request::builder()
        .uri(format!("/nix-cache/{hash}.narinfo"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_project_cache_serves_only_owned_persisted_narinfo() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project_a_name = format!("cache-a-{}", uuid::Uuid::new_v4().simple());
  let project_a = circus_common::repo::projects::create(
    &pool,
    circus_common::CreateProject {
      name:            project_a_name.clone(),
      repository_url:  "https://github.com/test/cache-a".to_string(),
      cache_enabled:   true,
      cache_url:       Some(format!(
        "https://ci.example.org/projects/{project_a_name}/nix-cache/"
      )),
      cache_upstreams: BinaryCacheUpstreams::default(),
      description:     None,
    },
  )
  .await
  .unwrap();
  let project_b_name = format!("cache-b-{}", uuid::Uuid::new_v4().simple());
  circus_common::repo::projects::create(&pool, circus_common::CreateProject {
    name:            project_b_name.clone(),
    repository_url:  "https://github.com/test/cache-b".to_string(),
    cache_enabled:   true,
    cache_url:       None,
    cache_upstreams: BinaryCacheUpstreams::default(),
    description:     None,
  })
  .await
  .unwrap();

  let hash = unique_nix_hash();
  let store_path = format!("/nix/store/{hash}-project-cache-test");
  circus_common::repo::narinfo_cache::upsert(
    &pool,
    circus_common::repo::narinfo_cache::UpsertNarInfo {
      store_path:  &store_path,
      nar_hash:
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      nar_size:    1,
      file_hash:   None,
      file_size:   None,
      compression: "none",
      url:         &format!("nar/{hash}.nar"),
      deriver:     None,
      references:  &[],
      sig:         Some("circus:test-signature"),
      ca:          None,
      build_id:    None,
      project_id:  Some(project_a.id),
    },
  )
  .await
  .unwrap();

  let config = circus_config::Config::default();
  let app = build_app_with_config(pool, &config);

  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri(format!(
          "/projects/{project_a_name}/nix-cache/{hash}.narinfo"
        ))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);

  let response = app
    .oneshot(
      Request::builder()
        .uri(format!(
          "/projects/{project_b_name}/nix-cache/{hash}.narinfo"
        ))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_project_cache_keeps_shared_narinfo_for_each_owner() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project_a_name = format!("shared-a-{}", uuid::Uuid::new_v4().simple());
  let project_a = circus_common::repo::projects::create(
    &pool,
    circus_common::CreateProject {
      name:            project_a_name.clone(),
      repository_url:  "https://github.com/test/shared-a".to_string(),
      cache_enabled:   true,
      cache_url:       None,
      cache_upstreams: BinaryCacheUpstreams::default(),
      description:     None,
    },
  )
  .await
  .unwrap();
  let project_b_name = format!("shared-b-{}", uuid::Uuid::new_v4().simple());
  let project_b = circus_common::repo::projects::create(
    &pool,
    circus_common::CreateProject {
      name:            project_b_name.clone(),
      repository_url:  "https://github.com/test/shared-b".to_string(),
      cache_enabled:   true,
      cache_url:       None,
      cache_upstreams: BinaryCacheUpstreams::default(),
      description:     None,
    },
  )
  .await
  .unwrap();

  let hash = unique_nix_hash();
  let store_path = format!("/nix/store/{hash}-shared-cache-test");
  let references = Vec::new();
  for project_id in [project_a.id, project_b.id] {
    circus_common::repo::narinfo_cache::upsert(
      &pool,
      circus_common::repo::narinfo_cache::UpsertNarInfo {
        store_path:  &store_path,
        nar_hash:
          "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        nar_size:    1,
        file_hash:   None,
        file_size:   None,
        compression: "none",
        url:         &format!("nar/{hash}.nar"),
        deriver:     None,
        references:  &references,
        sig:         Some("circus:test-signature"),
        ca:          None,
        build_id:    None,
        project_id:  Some(project_id),
      },
    )
    .await
    .unwrap();
  }

  let config = circus_config::Config::default();
  let app = build_app_with_config(pool, &config);

  for project_name in [project_a_name, project_b_name] {
    let response = app
      .clone()
      .oneshot(
        Request::builder()
          .uri(format!("/projects/{project_name}/nix-cache/{hash}.narinfo"))
          .body(Body::empty())
          .unwrap(),
      )
      .await
      .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
  }
}

#[tokio::test]
async fn test_cache_narinfo_local_url_requires_local_store_path() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let temp_root = std::env::temp_dir().join(format!(
    "circus-cache-missing-store-{}",
    uuid::Uuid::new_v4().simple()
  ));
  let store_dir = temp_root.join("store");
  tokio::fs::create_dir_all(&store_dir).await.unwrap();

  let store_hash = unique_nix_hash();
  let nar_hash_bare = "0".repeat(52);
  let nar_hash = format!("sha256:{nar_hash_bare}");
  let store_path = store_dir.join(format!("{store_hash}-missing-cache-test"));
  let store_path = store_path.to_string_lossy().to_string();

  circus_common::repo::narinfo_cache::upsert(
    &pool,
    circus_common::repo::narinfo_cache::UpsertNarInfo {
      store_path:  &store_path,
      nar_hash:    &nar_hash,
      nar_size:    10,
      file_hash:   Some(&nar_hash),
      file_size:   Some(10),
      compression: "none",
      url:         &format!("nar/{nar_hash_bare}.nar?hash={store_hash}"),
      deriver:     None,
      references:  &[],
      sig:         Some("circus:test-signature"),
      ca:          None,
      build_id:    None,
      project_id:  None,
    },
  )
  .await
  .unwrap();

  let mut config = circus_config::Config::default();
  config.cache.enabled = true;
  config.nix.store_dir = store_dir;
  let app = build_app_with_config(pool, &config);
  let response = app
    .oneshot(
      Request::builder()
        .uri(format!("/nix-cache/{store_hash}.narinfo"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::NOT_FOUND);
  let _ = tokio::fs::remove_dir_all(&temp_root).await;
}

#[tokio::test]
async fn test_cache_nar_invalid_hash_returns_404() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let mut config = circus_config::Config::default();
  config.cache.enabled = true;
  let app = build_app_with_config(pool, &config);

  // Invalid hash in NAR endpoint
  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/nix-cache/nar/INVALID_HASH.nar.zst")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);

  // Invalid hash in uncompressed NAR endpoint
  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/nix-cache/nar/INVALID_HASH.nar")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_cache_nar_route_accepts_signed_persisted_narinfo_provenance() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let temp_root = std::env::temp_dir().join(format!(
    "circus-cache-store-{}",
    uuid::Uuid::new_v4().simple()
  ));
  let store_dir = temp_root.join("store");
  let db_dir = temp_root.join("var/nix/db");
  tokio::fs::create_dir_all(&store_dir).await.unwrap();
  tokio::fs::create_dir_all(&db_dir).await.unwrap();

  let store_hash = "0123456789abcdfghijklmnpqrsvwxyz";
  let nar_hash_bare = "0".repeat(52);
  let nar_hash = format!("sha256:{nar_hash_bare}");
  let store_path = store_dir.join(format!("{store_hash}-signed-cache-test"));
  tokio::fs::write(&store_path, b"cache test").await.unwrap();
  let store_path = store_path.to_string_lossy().to_string();

  let sqlite = tokio_rusqlite::Connection::open(db_dir.join("db.sqlite"))
    .await
    .unwrap();
  let db_store_path = store_path.clone();
  let db_nar_hash = nar_hash.clone();
  sqlite
    .call(move |conn| -> tokio_rusqlite::rusqlite::Result<()> {
      conn.execute_batch(
        "CREATE TABLE ValidPaths (
           id INTEGER PRIMARY KEY,
           path TEXT NOT NULL,
           hash TEXT NOT NULL,
           registrationTime INTEGER NOT NULL,
           deriver TEXT,
           narSize INTEGER,
           ultimate INTEGER,
           sigs TEXT,
           ca TEXT
         );
         CREATE TABLE Refs (
           referrer INTEGER NOT NULL,
           reference INTEGER NOT NULL
         );",
      )?;
      conn.execute(
        "INSERT INTO ValidPaths (id, path, hash, registrationTime, deriver, \
         narSize, ultimate, sigs, ca) VALUES (1, ?1, ?2, 1, NULL, 10, 0, \
         NULL, NULL)",
        tokio_rusqlite::params![db_store_path, db_nar_hash],
      )?;
      Ok(())
    })
    .await
    .unwrap();
  sqlite.close().await.unwrap();

  circus_common::repo::narinfo_cache::upsert(
    &pool,
    circus_common::repo::narinfo_cache::UpsertNarInfo {
      store_path:  &store_path,
      nar_hash:    &nar_hash,
      nar_size:    10,
      file_hash:   Some(&nar_hash),
      file_size:   Some(10),
      compression: "none",
      url:         &format!("nar/{nar_hash_bare}.nar?hash={store_hash}"),
      deriver:     None,
      references:  &[],
      sig:         Some("circus:test-signature"),
      ca:          None,
      build_id:    None,
      project_id:  None,
    },
  )
  .await
  .unwrap();

  let mut config = circus_config::Config::default();
  config.cache.enabled = true;
  config.nix.store_dir = store_dir;
  let app = build_app_with_config(pool, &config);

  let response = app
    .oneshot(
      Request::builder()
        .uri(format!(
          "/nix-cache/nar/{nar_hash_bare}.nar?hash={store_hash}"
        ))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
  let _ = tokio::fs::remove_dir_all(&temp_root).await;
}

#[tokio::test]
async fn test_cache_disabled_returns_404() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let mut config = circus_config::Config::default();
  config.cache.enabled = false;
  let app = build_app_with_config(pool, &config);

  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/nix-cache/nix-cache-info")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);

  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/nix-cache")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);

  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/nix-cache/")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);

  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/nix-cache/abcdefghijklmnopqrstuvwxyz012345.narinfo")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_search_rejects_long_query() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app_public_reads(pool);

  // Query over 256 chars should return empty results
  let long_query = "a".repeat(300);
  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri(format!("/api/v1/search?q={long_query}"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
  assert_eq!(json["projects"], serde_json::json!([]));
  assert_eq!(json["builds"], serde_json::json!([]));
}

#[tokio::test]
async fn test_search_rejects_empty_query() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app_public_reads(pool);

  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/api/v1/search?q=")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
  assert_eq!(json["projects"], serde_json::json!([]));
  assert_eq!(json["builds"], serde_json::json!([]));
}

#[tokio::test]
async fn test_search_whitespace_only_query() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app_public_reads(pool);

  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/api/v1/search?q=%20%20%20")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
  assert_eq!(json["projects"], serde_json::json!([]));
}

#[tokio::test]
async fn test_builds_list_with_system_filter() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app_public_reads(pool);

  // Filter by system - should return 200 even with no results
  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/api/v1/builds?system=x86_64-linux")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
  assert!(json["items"].is_array());
  assert!(json["total"].is_number());
}

#[tokio::test]
async fn test_builds_list_with_job_name_filter() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app_public_reads(pool);

  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/api/v1/builds?job_name=hello")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
  assert!(json["items"].is_array());
}

#[tokio::test]
async fn test_builds_list_combined_filters() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app_public_reads(pool);

  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/api/v1/builds?system=aarch64-linux&status=pending&job_name=foo")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_cache_info_returns_correct_headers() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let mut config = circus_config::Config::default();
  config.cache.enabled = true;
  let app = build_app_with_config(pool, &config);

  for uri in ["/nix-cache", "/nix-cache/", "/nix-cache/nix-cache-info"] {
    let response = app
      .clone()
      .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
      .await
      .unwrap();

    assert_eq!(response.status(), StatusCode::OK, "{uri}");
    assert_eq!(
      response.headers().get("content-type").unwrap(),
      "text/plain"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
      .await
      .unwrap();
    let body_str = String::from_utf8(body.to_vec()).unwrap();
    assert!(body_str.contains("StoreDir: /nix/store"));
    assert!(body_str.contains("WantMassQuery: 1"));
    assert!(body_str.contains("Priority: 30"));
  }
}

#[tokio::test]
async fn test_channel_binary_cache_url_uses_active_project_cache_when_global_disabled()
 {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project_name = format!("project-cache-{}", uuid::Uuid::new_v4().simple());
  let channel_name = format!("channel-cache-{}", uuid::Uuid::new_v4().simple());
  let project = circus_common::repo::projects::create(
    &pool,
    circus_common::CreateProject {
      name:            project_name.clone(),
      repository_url:  "https://github.com/test/project-cache".to_string(),
      cache_enabled:   true,
      cache_url:       None,
      cache_upstreams: BinaryCacheUpstreams::default(),
      description:     None,
    },
  )
  .await
  .unwrap();
  let jobset =
    circus_common::repo::jobsets::create(&pool, circus_common::CreateJobset {
      project_id:        project.id,
      name:              "default".to_string(),
      nix_expression:    "packages".to_string(),
      enabled:           Some(true),
      flake_mode:        Some(true),
      check_interval:    Some(300),
      trigger_mode:      None,
      branch:            None,
      branch_pattern:    None,
      tag_pattern:       None,
      scheduling_shares: None,
      state:             None,
      keep_nr:           None,
      systems:           None,
    })
    .await
    .unwrap();
  circus_common::repo::channels::create(&pool, circus_common::CreateChannel {
    project_id: project.id,
    name:       channel_name.clone(),
    jobset_id:  jobset.id,
  })
  .await
  .unwrap();

  let mut config = circus_config::Config::default();
  config.cache.enabled = false;
  config.cache.cache_url =
    Some("https://ci.example.org/nix-cache/".to_string());
  let app = build_app_with_config(pool, &config);

  let response = app
    .oneshot(
      Request::builder()
        .uri(format!("/channel/{channel_name}/binary-cache-url"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  assert_eq!(
    String::from_utf8(body.to_vec()).unwrap(),
    format!("https://ci.example.org/projects/{project_name}/nix-cache/")
  );
}

#[tokio::test]
async fn test_metrics_endpoint() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app_with_admin_key(pool).await;

  let response = app
    .oneshot(
      Request::builder()
        .uri("/prometheus")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
  assert!(
    response
      .headers()
      .get("content-type")
      .unwrap()
      .to_str()
      .unwrap()
      .contains("text/plain")
  );

  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let body_str = String::from_utf8(body.to_vec()).unwrap();

  // Verify metric names are present
  assert!(body_str.contains("circus_builds_total"));
  assert!(body_str.contains("circus_projects_total"));
  assert!(body_str.contains("circus_evaluations_total"));

  // Verify Prometheus format: HELP/TYPE headers and label syntax
  assert!(
    body_str.contains("# HELP circus_builds_total"),
    "Missing HELP header for circus_builds_total"
  );
  assert!(
    body_str.contains("# TYPE circus_builds_total gauge"),
    "Missing TYPE header for circus_builds_total"
  );
  assert!(
    body_str.contains("circus_builds_total{status=\"succeeded\"}"),
    "Missing succeeded status label"
  );
  assert!(
    body_str.contains("circus_builds_total{status=\"failed\"}"),
    "Missing failed status label"
  );
  assert!(
    body_str.contains("circus_queue_depth"),
    "Missing queue depth metric"
  );
  assert!(
    body_str.contains("circus_builds_avg_duration_seconds"),
    "Missing avg duration metric"
  );

  // Verify each line with a metric value ends with a number (basic format
  // check)
  for line in body_str.lines() {
    if line.starts_with('#') || line.is_empty() {
      continue;
    }
    // Metric lines should have format: metric_name{labels} value
    // or: metric_name value
    let parts: Vec<&str> = line.rsplitn(2, ' ').collect();
    assert!(
      parts.len() == 2,
      "Malformed metric line (expected 'name value'): {line}"
    );
    assert!(
      parts[0].parse::<f64>().is_ok(),
      "Metric value is not a number: '{}' in line: {line}",
      parts[0]
    );
  }
}

#[tokio::test]
async fn test_get_nonexistent_build_returns_error_code() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app_with_admin_key(pool).await;
  let fake_id = uuid::Uuid::new_v4();

  let response = app
    .oneshot(
      Request::builder()
        .uri(format!("/api/v1/builds/{fake_id}"))
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::NOT_FOUND);

  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
  assert_eq!(json["error_code"], "NOT_FOUND");
  assert!(json["error"].as_str().unwrap().contains("not found"));
}

// Input validation

#[tokio::test]
async fn test_create_project_validation_rejects_invalid_name() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app_with_admin_key(pool).await;

  // Name starting with dash
  let body = serde_json::json!({
      "name": "-bad-name",
      "repository_url": "https://github.com/test/repo"
  });

  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/api/v1/projects")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
  assert_eq!(json["error_code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn test_create_project_validation_rejects_bad_url() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app_with_admin_key(pool).await;

  let body = serde_json::json!({
      "name": "valid-name",
      "repository_url": "ftp://bad-protocol.com/repo"
  });

  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/api/v1/projects")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
  assert_eq!(json["error_code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn test_create_project_validation_accepts_valid() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app_with_admin_key(pool).await;

  let body = serde_json::json!({
      "name": format!("valid-project-{}", uuid::Uuid::new_v4()),
      "repository_url": "https://github.com/test/repo",
      "description": "A valid project"
  });

  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/api/v1/projects")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
}

// Auth and error handling

#[tokio::test]
async fn test_project_create_with_auth() {
  use sha2::Digest;

  let Some(pool) = get_pool().await else {
    return;
  };

  // Create an admin API key
  let mut hasher = sha2::Sha256::new();
  hasher.update(b"circus_test_project_auth");
  let key_hash = hex::encode(hasher.finalize());
  let _ = circus_common::repo::api_keys::upsert(
    &pool,
    "test-auth",
    &key_hash,
    circus_common::roles::GlobalRole::Admin,
  )
  .await;

  let app = build_app(pool);

  let body = serde_json::json!({
      "name": "auth-test-project",
      "repository_url": "https://github.com/test/auth-test"
  });

  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/api/v1/projects")
        .header("content-type", "application/json")
        .header("authorization", "Bearer circus_test_project_auth")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);

  let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
  assert_eq!(json["name"], "auth-test-project");
  assert!(json["id"].as_str().is_some());
}

#[tokio::test]
async fn test_project_create_without_auth_rejected() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app(pool);

  let body = serde_json::json!({
      "name": "no-auth-project",
      "repository_url": "https://github.com/test/no-auth"
  });

  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/api/v1/projects")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_api_reads_require_auth_by_default() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app(pool);
  let response = app
    .oneshot(
      Request::builder()
        .uri("/api/v1/builds/recent")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_probe_requires_auth_and_rejects_bad_scheme() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app_with_admin_key(pool).await;
  let body = serde_json::json!({ "repository_url": "path:/etc/passwd" });

  let response = app
    .clone()
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/api/v1/projects/probe")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/api/v1/projects/probe")
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_webhook_empty_secret_rejected() {
  let Some(pool) = get_pool().await else {
    return;
  };

  ensure_api_key(&pool, ADMIN_TOKEN, circus_common::roles::GlobalRole::Admin)
    .await;
  let project_id = create_test_project(&pool).await;
  let mut config = circus_config::Config::default();
  config.server.webhook_secret_encryption_key = Some("test-key".into());
  let app = build_app_with_config(pool, &config);

  let body = serde_json::json!({ "forge_type": "github", "secret": "" });
  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri(format!("/api/v1/projects/{project_id}/webhooks"))
        .header("content-type", "application/json")
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_webhook_without_secret_is_not_configured() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project_id = create_test_project(&pool).await;
  // repo::webhook_configs has no NULL-secret path on purpose, so seed the
  // misconfigured row directly.
  let client = pool.get().await.unwrap();
  client
    .execute(
      "INSERT INTO webhook_configs (project_id, forge_type, secret_hash) \
       VALUES ($1, 'github', NULL)",
      &[&project_id],
    )
    .await
    .unwrap();
  drop(client);

  let app = build_app(pool);
  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri(format!("/api/v1/webhooks/{project_id}/github"))
        .header("x-github-event", "push")
        .body(Body::from(r#"{"ref":"refs/heads/main","after":"abc"}"#))
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_admin_reads_require_admin() {
  let Some(pool) = get_pool().await else {
    return;
  };

  ensure_api_key(
    &pool,
    READ_TOKEN,
    circus_common::roles::GlobalRole::ReadOnly,
  )
  .await;
  let app = build_app(pool);
  let response = app
    .oneshot(
      Request::builder()
        .uri("/api/v1/admin/builders")
        .header("authorization", format!("Bearer {READ_TOKEN}"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn test_dashboard_build_log_requires_auth_by_default() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app(pool);
  let response = app
    .oneshot(
      Request::builder()
        .uri(format!("/build/{}/log", uuid::Uuid::new_v4()))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_setup_endpoint_creates_project_and_jobsets() {
  use sha2::Digest;

  let Some(pool) = get_pool().await else {
    return;
  };

  // Create an admin API key
  let mut hasher = sha2::Sha256::new();
  hasher.update(b"circus_test_setup_key");
  let key_hash = hex::encode(hasher.finalize());
  let _ = circus_common::repo::api_keys::upsert(
    &pool,
    "test-setup",
    &key_hash,
    circus_common::roles::GlobalRole::Admin,
  )
  .await;

  let app = build_app(pool.clone());

  let body = serde_json::json!({
      "repository_url": "https://github.com/test/setup-test",
      "name": "setup-test-project",
      "description": "Test project from setup endpoint",
      "jobsets": [
          {
              "name": "packages",
              "nix_expression": "packages",
              "description": "Packages"
          },
          {
              "name": "checks",
              "nix_expression": "checks",
              "description": "Checks"
          }
      ]
  });

  let response = app
    .oneshot(
      Request::builder()
        .method("POST")
        .uri("/api/v1/projects/setup")
        .header("content-type", "application/json")
        .header("authorization", "Bearer circus_test_setup_key")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);

  let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

  assert_eq!(json["project"]["name"], "setup-test-project");
  assert_eq!(json["jobsets"].as_array().unwrap().len(), 2);
  assert_eq!(json["jobsets"][0]["name"], "packages");
  assert_eq!(json["jobsets"][1]["name"], "checks");
}

#[tokio::test]
async fn test_security_headers_present() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app(pool);

  let response = app
    .oneshot(
      Request::builder()
        .uri("/health")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(
    response
      .headers()
      .get("x-content-type-options")
      .map(|v| v.to_str().unwrap()),
    Some("nosniff")
  );
  assert_eq!(
    response
      .headers()
      .get("x-frame-options")
      .map(|v| v.to_str().unwrap()),
    Some("DENY")
  );
  assert_eq!(
    response
      .headers()
      .get("referrer-policy")
      .map(|v| v.to_str().unwrap()),
    Some("strict-origin-when-cross-origin")
  );
}

#[tokio::test]
async fn test_static_css_served() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let app = build_app(pool);

  let response = app
    .oneshot(
      Request::builder()
        .uri("/static/style.css")
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
  assert_eq!(
    response
      .headers()
      .get("content-type")
      .map(|v| v.to_str().unwrap()),
    Some("text/css")
  );

  let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let css = String::from_utf8_lossy(&body_bytes);
  assert!(css.contains("--accent"), "CSS should contain design tokens");
  assert!(
    css.contains("prefers-color-scheme: dark"),
    "CSS should have dark mode"
  );
}

/// Seed one narinfo row scoped to a project with explicit sizes and a known
/// package-name suffix so the admin cache surfaces can be asserted exactly.
async fn seed_project_narinfo(
  pool: &circus_common::PgPool,
  project_id: uuid::Uuid,
  hash: &str,
  package: &str,
  nar_size: i64,
  file_size: i64,
) {
  let store_path = format!("/nix/store/{hash}-{package}");
  circus_common::repo::narinfo_cache::upsert(
    pool,
    circus_common::repo::narinfo_cache::UpsertNarInfo {
      store_path: &store_path,
      nar_hash: "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      nar_size,
      file_hash: None,
      file_size: Some(file_size),
      compression: "zstd",
      url: &format!("nar/{hash}.nar.zst"),
      deriver: None,
      references: &[],
      sig: Some("circus:test-signature"),
      ca: None,
      build_id: None,
      project_id: Some(project_id),
    },
  )
  .await
  .unwrap();
}

#[tokio::test]
async fn test_admin_cache_storage_summary() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let name = format!("cachesum-{}", uuid::Uuid::new_v4().simple());
  let project = circus_common::repo::projects::create(
    &pool,
    circus_common::CreateProject {
      name:            name.clone(),
      repository_url:  "https://github.com/test/cachesum".to_string(),
      cache_enabled:   true,
      cache_url:       None,
      cache_upstreams: BinaryCacheUpstreams::default(),
      description:     None,
    },
  )
  .await
  .unwrap();

  let hash = unique_nix_hash();
  seed_project_narinfo(&pool, project.id, &hash, "summary-pkg", 200, 100).await;

  let app = build_app_with_admin_key(pool).await;
  let response = app
    .oneshot(
      Request::builder()
        .uri(format!("/api/v1/admin/caches/{name}"))
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::OK);

  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
  assert_eq!(json["scope"], "project");
  assert_eq!(json["storage"]["nar_count"], 1);
  assert_eq!(json["storage"]["uncompressed_bytes"], 200);
  assert_eq!(json["storage"]["compressed_bytes"], 100);
}

#[tokio::test]
async fn test_admin_global_cache_includes_signed_local_build_products() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project_name = format!("local-cache-{}", uuid::Uuid::new_v4().simple());
  let project = circus_common::repo::projects::create(
    &pool,
    circus_common::CreateProject {
      name:            project_name,
      repository_url:  "https://github.com/test/local-cache".to_string(),
      cache_enabled:   true,
      cache_url:       None,
      cache_upstreams: BinaryCacheUpstreams::default(),
      description:     None,
    },
  )
  .await
  .unwrap();
  let jobset =
    circus_common::repo::jobsets::create(&pool, circus_common::CreateJobset {
      project_id:        project.id,
      name:              "default".to_string(),
      nix_expression:    "packages".to_string(),
      enabled:           Some(true),
      flake_mode:        Some(true),
      check_interval:    Some(300),
      trigger_mode:      None,
      branch:            None,
      branch_pattern:    None,
      tag_pattern:       None,
      scheduling_shares: None,
      state:             None,
      keep_nr:           None,
      systems:           None,
    })
    .await
    .unwrap();
  let evaluation = circus_common::repo::evaluations::create(
    &pool,
    circus_common::CreateEvaluation {
      jobset_id:      jobset.id,
      commit_hash:    uuid::Uuid::new_v4().simple().to_string(),
      pr_number:      None,
      pr_head_branch: None,
      pr_base_branch: None,
      pr_action:      None,
    },
  )
  .await
  .unwrap();
  let hash = unique_nix_hash();
  let store_path = format!("/nix/store/{hash}-local-cache-pkg");
  let build =
    circus_common::repo::builds::create(&pool, circus_common::CreateBuild {
      evaluation_id: evaluation.id,
      job_name: "local-cache-pkg".to_string(),
      drv_path: format!("/nix/store/{hash}-local-cache-pkg.drv"),
      system: Some("x86_64-linux".to_string()),
      outputs: Some(serde_json::json!({ "out": store_path })),
      ..Default::default()
    })
    .await
    .unwrap();
  circus_common::repo::builds::complete(
    &pool,
    build.id,
    circus_common::BuildStatus::Succeeded,
    None,
    Some(&store_path),
    None,
  )
  .await
  .unwrap();
  circus_common::repo::builds::mark_signed(&pool, build.id)
    .await
    .unwrap();
  circus_common::repo::build_products::create(
    &pool,
    circus_common::CreateBuildProduct {
      build_id:     build.id,
      name:         "local-cache-pkg".to_string(),
      path:         store_path.clone(),
      sha256_hash:  Some(
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
          .to_string(),
      ),
      file_size:    Some(1234),
      content_type: None,
      is_directory: true,
    },
  )
  .await
  .unwrap();

  let app = build_app_with_admin_key(pool).await;
  let detail = app
    .clone()
    .oneshot(
      Request::builder()
        .uri("/api/v1/admin/caches/global")
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(detail.status(), StatusCode::OK);
  let body = axum::body::to_bytes(detail.into_body(), usize::MAX)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
  assert!(
    json["storage"]["nar_count"].as_i64().unwrap() >= 1,
    "{json}"
  );
  assert!(
    json["storage"]["uncompressed_bytes"].as_i64().unwrap() >= 1234,
    "{json}"
  );

  let nars = app
    .clone()
    .oneshot(
      Request::builder()
        .uri(format!("/api/v1/admin/caches/global/nars?hash={hash}"))
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(nars.status(), StatusCode::OK);
  let body = axum::body::to_bytes(nars.into_body(), usize::MAX)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
  assert_eq!(json["total"], 1);
  assert_eq!(json["items"][0]["store_path"], store_path);
  assert_eq!(json["items"][0]["package_name"], "local-cache-pkg");

  let series = app
    .oneshot(
      Request::builder()
        .uri("/api/v1/admin/caches/global/storage-timeseries?granularity=hours")
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(series.status(), StatusCode::OK);
  let body = axum::body::to_bytes(series.into_body(), usize::MAX)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
  let bytes_added = json["bytes_added"]
    .as_array()
    .unwrap()
    .iter()
    .filter_map(serde_json::Value::as_i64)
    .sum::<i64>();
  assert!(bytes_added >= 1234, "{json}");
}

#[tokio::test]
async fn test_project_cache_includes_local_product_when_other_project_uploaded_same_path()
 {
  let Some(pool) = get_pool().await else {
    return;
  };

  let project_a_name =
    format!("uploaded-owner-{}", uuid::Uuid::new_v4().simple());
  let project_a = circus_common::repo::projects::create(
    &pool,
    circus_common::CreateProject {
      name:            project_a_name,
      repository_url:  "https://github.com/test/uploaded-owner".to_string(),
      cache_enabled:   true,
      cache_url:       None,
      cache_upstreams: BinaryCacheUpstreams::default(),
      description:     None,
    },
  )
  .await
  .unwrap();
  let project_b_name = format!("local-owner-{}", uuid::Uuid::new_v4().simple());
  let project_b = circus_common::repo::projects::create(
    &pool,
    circus_common::CreateProject {
      name:            project_b_name.clone(),
      repository_url:  "https://github.com/test/local-owner".to_string(),
      cache_enabled:   true,
      cache_url:       None,
      cache_upstreams: BinaryCacheUpstreams::default(),
      description:     None,
    },
  )
  .await
  .unwrap();

  let hash = unique_nix_hash();
  let store_path = format!("/nix/store/{hash}-shared-cache-pkg");
  circus_common::repo::narinfo_cache::upsert(
    &pool,
    circus_common::repo::narinfo_cache::UpsertNarInfo {
      store_path:  &store_path,
      nar_hash:
        "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
      nar_size:    10,
      file_hash:   None,
      file_size:   Some(5),
      compression: "zstd",
      url:         &format!("nar/{hash}.nar.zst"),
      deriver:     None,
      references:  &[],
      sig:         Some("circus:test-signature"),
      ca:          None,
      build_id:    None,
      project_id:  Some(project_a.id),
    },
  )
  .await
  .unwrap();

  let jobset =
    circus_common::repo::jobsets::create(&pool, circus_common::CreateJobset {
      project_id:        project_b.id,
      name:              "default".to_string(),
      nix_expression:    "packages".to_string(),
      enabled:           Some(true),
      flake_mode:        Some(true),
      check_interval:    Some(300),
      trigger_mode:      None,
      branch:            None,
      branch_pattern:    None,
      tag_pattern:       None,
      scheduling_shares: None,
      state:             None,
      keep_nr:           None,
      systems:           None,
    })
    .await
    .unwrap();
  let evaluation = circus_common::repo::evaluations::create(
    &pool,
    circus_common::CreateEvaluation {
      jobset_id:      jobset.id,
      commit_hash:    uuid::Uuid::new_v4().simple().to_string(),
      pr_number:      None,
      pr_head_branch: None,
      pr_base_branch: None,
      pr_action:      None,
    },
  )
  .await
  .unwrap();
  let build =
    circus_common::repo::builds::create(&pool, circus_common::CreateBuild {
      evaluation_id: evaluation.id,
      job_name: "shared-cache-pkg".to_string(),
      drv_path: format!("/nix/store/{hash}-shared-cache-pkg.drv"),
      system: Some("x86_64-linux".to_string()),
      outputs: Some(serde_json::json!({ "out": store_path })),
      ..Default::default()
    })
    .await
    .unwrap();
  circus_common::repo::builds::complete(
    &pool,
    build.id,
    circus_common::BuildStatus::Succeeded,
    None,
    Some(&store_path),
    None,
  )
  .await
  .unwrap();
  circus_common::repo::builds::mark_signed(&pool, build.id)
    .await
    .unwrap();
  circus_common::repo::build_products::create(
    &pool,
    circus_common::CreateBuildProduct {
      build_id:     build.id,
      name:         "shared-cache-pkg".to_string(),
      path:         store_path.clone(),
      sha256_hash:  Some(
        "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
      ),
      file_size:    Some(1234),
      content_type: None,
      is_directory: true,
    },
  )
  .await
  .unwrap();

  let app = build_app_with_admin_key(pool).await;
  let response = app
    .oneshot(
      Request::builder()
        .uri(format!(
          "/api/v1/admin/caches/{project_b_name}/nars?hash={hash}"
        ))
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();

  assert_eq!(response.status(), StatusCode::OK);
  let body = axum::body::to_bytes(response.into_body(), usize::MAX)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
  assert_eq!(json["total"], 1);
  assert_eq!(json["items"][0]["store_path"], store_path);
  assert_eq!(json["items"][0]["package_name"], "shared-cache-pkg");
}

#[tokio::test]
async fn test_admin_cache_nars_filter_by_hash_and_package() {
  let Some(pool) = get_pool().await else {
    return;
  };

  let name = format!("cachenars-{}", uuid::Uuid::new_v4().simple());
  let project = circus_common::repo::projects::create(
    &pool,
    circus_common::CreateProject {
      name:            name.clone(),
      repository_url:  "https://github.com/test/cachenars".to_string(),
      cache_enabled:   true,
      cache_url:       None,
      cache_upstreams: BinaryCacheUpstreams::default(),
      description:     None,
    },
  )
  .await
  .unwrap();

  // Two distinct hash prefixes and package names within this project's scope.
  let hash_foo = format!("aaaa{}", uuid::Uuid::new_v4().simple());
  let hash_bar = format!("bbbb{}", uuid::Uuid::new_v4().simple());
  seed_project_narinfo(&pool, project.id, &hash_foo, "foopkg", 10, 5).await;
  seed_project_narinfo(&pool, project.id, &hash_bar, "barpkg", 20, 7).await;

  let app = build_app_with_admin_key(pool).await;

  // Filter by package name: only the foo row matches.
  let by_pkg = app
    .clone()
    .oneshot(
      Request::builder()
        .uri(format!("/api/v1/admin/caches/{name}/nars?package=foopkg"))
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(by_pkg.status(), StatusCode::OK);
  let body = axum::body::to_bytes(by_pkg.into_body(), usize::MAX)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
  assert_eq!(json["total"], 1);
  assert_eq!(json["items"][0]["package_name"], "foopkg");

  // Filter by hash prefix: only the bar row matches.
  let by_hash = app
    .oneshot(
      Request::builder()
        .uri(format!("/api/v1/admin/caches/{name}/nars?hash=bbbb"))
        .header("authorization", format!("Bearer {ADMIN_TOKEN}"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(by_hash.status(), StatusCode::OK);
  let body = axum::body::to_bytes(by_hash.into_body(), usize::MAX)
    .await
    .unwrap();
  let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
  assert_eq!(json["total"], 1);
  assert_eq!(json["items"][0]["package_name"], "barpkg");
}

#[tokio::test]
async fn test_admin_caches_forbidden_for_non_admin() {
  let Some(pool) = get_pool().await else {
    return;
  };

  ensure_api_key(
    &pool,
    READ_TOKEN,
    circus_common::roles::GlobalRole::ReadOnly,
  )
  .await;
  let app = build_app(pool);

  let response = app
    .oneshot(
      Request::builder()
        .uri("/api/v1/admin/caches")
        .header("authorization", format!("Bearer {READ_TOKEN}"))
        .body(Body::empty())
        .unwrap(),
    )
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
