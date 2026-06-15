pub mod admin;
pub mod auth;
pub mod badges;
pub mod builds;
pub mod cache;
pub mod channel_manifests;
pub mod channels;
pub mod dashboard;
pub mod evaluations;
pub mod health;
pub mod jobsets;
pub mod ldap;
pub mod logs;
pub mod metrics;
pub mod news;
pub mod oauth;
pub mod openapi;
pub mod operator;
pub mod projects;
pub mod search;
pub(crate) mod serde_util;
pub mod users;
pub mod webhooks;

use std::{
  net::IpAddr,
  path::{Path, PathBuf},
  sync::Arc,
  time::Instant,
};

use axum::{
  Router,
  body::Body,
  extract::ConnectInfo,
  http::{HeaderValue, Request, StatusCode, header},
  middleware::{self, Next},
  response::{IntoResponse, Response},
  routing::get,
};
use circus_config::ServerConfig;
use dashmap::DashMap;
use tower_http::{
  cors::{AllowOrigin, Any, CorsLayer},
  limit::RequestBodyLimitLayer,
  set_header::SetResponseHeaderLayer,
  trace::TraceLayer,
};

use crate::{
  auth_middleware::{extract_session, require_api_key},
  state::AppState,
};

static STYLE_CSS: &str = include_str!("../../static/style.css");

pub(crate) async fn canonical_log_file(
  log_dir: &Path,
  path: &Path,
) -> Option<PathBuf> {
  let base = tokio::fs::canonicalize(log_dir).await.ok()?;
  let path = tokio::fs::canonicalize(path).await.ok()?;
  path.starts_with(base).then_some(path)
}

/// Per-IP token bucket. Tokens accrue at `rps` per second up to `burst`.
/// Each request costs one token; if the bucket is empty the request is
/// rejected with 429.
struct Bucket {
  tokens:        f64,
  last_refilled: Instant,
}

struct RateLimitState {
  buckets:      DashMap<IpAddr, Bucket>,
  rps:          f64,
  burst:        f64,
  last_cleanup: std::sync::Mutex<Instant>,
}

/// How long an idle bucket persists before the periodic sweep drops it.
const RATE_LIMIT_BUCKET_TTL: std::time::Duration =
  std::time::Duration::from_mins(5);

async fn rate_limit_middleware(
  ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
  request: Request<axum::body::Body>,
  next: Next,
) -> Response {
  let state = request.extensions().get::<Arc<RateLimitState>>().cloned();

  if let Some(rl) = state {
    let ip = addr.ip();
    let now = Instant::now();

    // Periodic cleanup of idle buckets (every 60s, Instant-based so a
    // wall-clock step doesn't strand us).
    {
      let mut last = rl
        .last_cleanup
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
      if now.duration_since(*last) > std::time::Duration::from_mins(1) {
        *last = now;
        rl.buckets.retain(|_, b| {
          now.duration_since(b.last_refilled) < RATE_LIMIT_BUCKET_TTL
        });
      }
      drop(last);
    }

    let mut entry = rl.buckets.entry(ip).or_insert_with(|| {
      Bucket {
        tokens:        rl.burst,
        last_refilled: now,
      }
    });

    let elapsed = now.duration_since(entry.last_refilled).as_secs_f64();
    entry.tokens = elapsed.mul_add(rl.rps, entry.tokens).min(rl.burst);
    entry.last_refilled = now;

    if entry.tokens < 1.0 {
      return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    entry.tokens -= 1.0;
    drop(entry);
  }

  next.run(request).await
}

async fn serve_style_css() -> Response {
  #[expect(
    clippy::expect_used,
    reason = "response builder with static values cannot fail"
  )]
  {
    Response::builder()
      .header(header::CONTENT_TYPE, "text/css")
      .header(header::CACHE_CONTROL, "public, max-age=3600")
      .body(Body::from(STYLE_CSS))
      .expect("response builder should not fail")
  }
  .into_response()
}

pub fn api_router(state: AppState) -> Router<AppState> {
  Router::new()
    .merge(projects::router())
    .merge(jobsets::router())
    .merge(evaluations::router())
    .merge(builds::router())
    .merge(logs::router())
    .merge(auth::router())
    .merge(users::router())
    .merge(search::router())
    .merge(channels::router())
    .merge(news::router())
    .merge(admin::router())
    .merge(operator::router())
    .route_layer(middleware::from_fn_with_state(state, require_api_key))
}

pub fn public_router() -> Router<AppState> {
  Router::new()
    .merge(health::router())
    .merge(badges::router())
    .merge(cache::router())
    .merge(channel_manifests::router())
    .merge(openapi::router())
    .merge(metrics::router())
    // Webhooks use their own HMAC auth, outside the API key gate.
    .merge(webhooks::router())
    // OAuth and LDAP routes use their own auth mechanisms.
    .merge(oauth::router())
    .merge(ldap::router())
}

pub fn ui_router(state: AppState) -> Router<AppState> {
  Router::new()
    .route("/static/style.css", get(serve_style_css))
    .merge(
      dashboard::router()
        .route_layer(middleware::from_fn_with_state(state, extract_session)),
    )
}

pub fn router(state: AppState, config: &ServerConfig) -> Router {
  let cors_layer = if config.cors_permissive {
    tracing::warn!(
      "server.cors_permissive is enabled; CORS credentials are disabled"
    );
    CorsLayer::new()
      .allow_origin(Any)
      .allow_methods(Any)
      .allow_headers(Any)
      .allow_credentials(false)
  } else if config.allowed_origins.is_empty() {
    CorsLayer::new()
  } else {
    let origins: Vec<HeaderValue> = config
      .allowed_origins
      .iter()
      .filter_map(|o| o.parse().ok())
      .collect();
    CorsLayer::new().allow_origin(AllowOrigin::list(origins))
  };

  let mut app = Router::new()
    .nest("/api/v1", api_router(state.clone()))
    .merge(public_router());

  if config.ui_enabled {
    app = app.merge(ui_router(state.clone()));
  }

  app = app
        .layer(TraceLayer::new_for_http())
        .layer(cors_layer)
        .layer(RequestBodyLimitLayer::new(config.max_body_size))
        // Security headers
        .layer(SetResponseHeaderLayer::overriding(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::REFERRER_POLICY,
            HeaderValue::from_static("strict-origin-when-cross-origin"),
        ));

  // Add rate limiting if configured
  if let (Some(rps), Some(burst)) =
    (config.rate_limit_rps, config.rate_limit_burst)
  {
    let rl_state = Arc::new(RateLimitState {
      buckets:      DashMap::new(),
      rps:          rps as f64,
      burst:        f64::from(burst),
      last_cleanup: std::sync::Mutex::new(Instant::now()),
    });
    app = app
      .layer(axum::Extension(rl_state))
      .layer(middleware::from_fn(rate_limit_middleware));
  }

  app.with_state(state)
}
