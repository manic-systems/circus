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
pub mod oauth;
pub mod openapi;
pub mod projects;
pub mod search;
pub mod users;
pub mod webhooks;

use std::{net::IpAddr, sync::Arc, time::Instant};

use axum::{
  Router,
  body::Body,
  extract::ConnectInfo,
  http::{HeaderValue, Request, StatusCode, header},
  middleware::{self, Next},
  response::{IntoResponse, Response},
  routing::get,
};
use circus_common::config::ServerConfig;
use dashmap::DashMap;
use tower_http::{
  cors::{AllowOrigin, CorsLayer},
  limit::RequestBodyLimitLayer,
  set_header::SetResponseHeaderLayer,
  trace::TraceLayer,
};

use crate::{
  auth_middleware::{extract_session, require_api_key},
  state::AppState,
};

static STYLE_CSS: &str = include_str!("../../static/style.css");

/// Helper to generate secure cookie flags based on server configuration.
/// Returns a string containing cookie security attributes: `HttpOnly`,
/// `SameSite`, and optionally Secure.
///
/// The Secure flag is set when:
///
/// 1. `force_secure_cookies` is enabled in config (for HTTPS reverse proxies),
/// 2. OR the server is not bound to localhost/127.0.0.1 AND not in permissive
///    mode
#[must_use]
pub fn cookie_security_flags(
  config: &circus_common::config::ServerConfig,
) -> String {
  let is_localhost = config.host == "127.0.0.1"
    || config.host == "localhost"
    || config.host == "::1";

  let secure_flag = if config.force_secure_cookies
    || (!is_localhost && !config.cors_permissive)
  {
    "; Secure"
  } else {
    ""
  };

  format!("HttpOnly; SameSite=Strict{secure_flag}")
}

struct RateLimitState {
  requests:     DashMap<IpAddr, Vec<Instant>>,
  rps:          u64,
  burst:        u32,
  last_cleanup: std::sync::atomic::AtomicU64,
}

async fn rate_limit_middleware(
  ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
  request: Request<axum::body::Body>,
  next: Next,
) -> Response {
  let state = request.extensions().get::<Arc<RateLimitState>>().cloned();

  if let Some(rl) = state {
    let ip = addr.ip();
    let now = Instant::now();
    let window = std::time::Duration::from_secs(1);

    // Periodic cleanup of stale entries (every 60 seconds)
    let now_secs = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .unwrap_or_default()
      .as_secs();
    let last = rl.last_cleanup.load(std::sync::atomic::Ordering::Relaxed);
    if now_secs - last > 60
      && rl
        .last_cleanup
        .compare_exchange(
          last,
          now_secs,
          std::sync::atomic::Ordering::SeqCst,
          std::sync::atomic::Ordering::Relaxed,
        )
        .is_ok()
    {
      rl.requests.retain(|_, v| {
        v.retain(|t| {
          now.duration_since(*t) < std::time::Duration::from_secs(10)
        });
        !v.is_empty()
      });
    }

    let mut entry = rl.requests.entry(ip).or_default();
    entry.retain(|t| now.duration_since(*t) < window);

    // Token bucket algorithm: allow burst, then enforce rps limit
    let request_count = entry.len();
    if request_count >= rl.burst as usize {
      return StatusCode::TOO_MANY_REQUESTS.into_response();
    }

    // If within burst but need to check rate, ensure we don't exceed rps
    if request_count >= rl.rps as usize {
      // Check if oldest request in window is still within the rps constraint
      if let Some(oldest) = entry.first() {
        let elapsed = now.duration_since(*oldest);
        if elapsed < window {
          return StatusCode::TOO_MANY_REQUESTS.into_response();
        }
      }
    }

    entry.push(now);
    drop(entry);
  }

  next.run(request).await
}

async fn serve_style_css() -> Response {
  Response::builder()
    .header(header::CONTENT_TYPE, "text/css")
    .header(header::CACHE_CONTROL, "public, max-age=3600")
    .body(Body::from(STYLE_CSS))
    .unwrap()
    .into_response()
}

pub fn router(state: AppState, config: &ServerConfig) -> Router {
  let cors_layer = if config.cors_permissive {
    CorsLayer::permissive()
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
        // Static assets
        .route("/static/style.css", get(serve_style_css))
        // Dashboard routes (SSR templates) with session extraction
        .merge(dashboard::router().route_layer(middleware::from_fn_with_state(
            state.clone(),
            extract_session,
        )))
        // API routes
        .nest(
            "/api/v1",
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
                .merge(admin::router())
                .route_layer(middleware::from_fn_with_state(
                    state.clone(),
                    require_api_key,
                )),
        )
        .merge(health::router())
        .merge(badges::router())
        .merge(cache::router())
        .merge(channel_manifests::router())
        .merge(openapi::router())
        .merge(metrics::router())
        // Webhooks use their own HMAC auth, outside the API key gate
        .merge(webhooks::router())
        // OAuth routes use their own auth mechanism
        .merge(oauth::router())
        // LDAP login (no API key, uses bind to authenticate)
        .merge(ldap::router())
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
      requests: DashMap::new(),
      rps,
      burst,
      last_cleanup: std::sync::atomic::AtomicU64::new(0),
    });
    app = app
      .layer(axum::Extension(rl_state))
      .layer(middleware::from_fn(rate_limit_middleware));
  }

  app.with_state(state)
}
