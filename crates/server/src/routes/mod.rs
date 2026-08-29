pub mod admin;
pub mod auth;
pub mod badges;
pub mod builds;
pub mod cache;
pub mod channel_manifests;
pub mod channels;
pub mod dashboard;
pub(crate) mod declarative;
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
  net::{IpAddr, SocketAddr},
  path::{Path, PathBuf},
  sync::{Arc, Mutex, PoisonError},
  time::{Duration, Instant},
};

use axum::{
  Router,
  body::Body,
  extract::{ConnectInfo, State},
  http::{HeaderValue, Request, StatusCode, header},
  middleware::{self, Next},
  response::{IntoResponse, Response},
  routing::get,
};
use circus_config::Config;
use dashmap::DashMap;
use tower_http::{
  cors::{AllowOrigin, Any, CorsLayer},
  limit::RequestBodyLimitLayer,
  services::ServeDir,
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
  last_cleanup: Mutex<Instant>,
}

/// How long an idle bucket persists before the periodic sweep drops it.
const RATE_LIMIT_BUCKET_TTL: Duration = Duration::from_mins(5);

async fn rate_limit_middleware(
  ConnectInfo(addr): ConnectInfo<SocketAddr>,
  request: Request<Body>,
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
        .unwrap_or_else(PoisonError::into_inner);
      if now.duration_since(*last) > Duration::from_mins(1) {
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
      // The stylesheet is compiled into the binary and cache-busted via the
      // `?v=` query string on its <link> (bump it when style.css changes), so
      // it is safe to cache immutably. Serving it `no-cache` forced the
      // browser to re-download and re-parse this render-blocking asset on
      // every navigation, which made each page feel a beat slow to load.
      .header(
        header::CACHE_CONTROL,
        "public, max-age=31536000, immutable",
      )
      .body(Body::from(STYLE_CSS))
      .expect("response builder should not fail")
  }
  .into_response()
}

async fn serve_theme_css(State(state): State<AppState>) -> Response {
  let mut css = String::from(":root {\n");
  for (name, value) in &state.config.ui.css_variables {
    css.push_str("  ");
    if !name.starts_with("--") {
      css.push_str("--");
    }
    css.push_str(name);
    css.push_str(": ");
    css.push_str(value);
    css.push_str(";\n");
  }
  append_derived_contrast(
    &mut css,
    &state.config.ui.css_variables,
    "accent",
    "accent-contrast",
  );
  append_derived_contrast(
    &mut css,
    &state.config.ui.css_variables,
    "accent-hover",
    "accent-hover-contrast",
  );
  css.push_str("}\n");

  #[expect(
    clippy::expect_used,
    reason = "response builder with static values cannot fail"
  )]
  {
    Response::builder()
      .header(header::CONTENT_TYPE, "text/css")
      .header(header::CACHE_CONTROL, "no-cache")
      .body(Body::from(css))
      .expect("response builder should not fail")
  }
  .into_response()
}

fn css_variable<'a>(
  variables: &'a std::collections::BTreeMap<String, String>,
  name: &str,
) -> Option<&'a str> {
  variables
    .get(name)
    .or_else(|| variables.get(&format!("--{name}")))
    .map(String::as_str)
}

fn append_derived_contrast(
  css: &mut String,
  variables: &std::collections::BTreeMap<String, String>,
  color_name: &str,
  contrast_name: &str,
) {
  if css_variable(variables, contrast_name).is_some() {
    return;
  }
  let Some(color) =
    css_variable(variables, color_name).and_then(accent_contrast_color)
  else {
    return;
  };
  css.push_str("  --");
  css.push_str(contrast_name);
  css.push_str(": ");
  css.push_str(color);
  css.push_str(";\n");
}

fn accent_contrast_color(color: &str) -> Option<&'static str> {
  let hex = color.trim().strip_prefix('#')?.as_bytes();
  let (r, g, b) = match hex {
    [r, g, b] => {
      (
        hex_nibble(*r)? * 17,
        hex_nibble(*g)? * 17,
        hex_nibble(*b)? * 17,
      )
    },
    [r1, r2, g1, g2, b1, b2] => {
      (
        hex_byte(*r1, *r2)?,
        hex_byte(*g1, *g2)?,
        hex_byte(*b1, *b2)?,
      )
    },
    _ => return None,
  };
  let yiq = u32::from(r) * 299 + u32::from(g) * 587 + u32::from(b) * 114;
  if yiq >= 128_000 {
    Some("#0b0f14")
  } else {
    Some("#ffffff")
  }
}

fn hex_byte(high: u8, low: u8) -> Option<u8> {
  Some(hex_nibble(high)? * 16 + hex_nibble(low)?)
}

const fn hex_nibble(byte: u8) -> Option<u8> {
  match byte {
    b'0'..=b'9' => Some(byte - b'0'),
    b'a'..=b'f' => Some(byte - b'a' + 10),
    b'A'..=b'F' => Some(byte - b'A' + 10),
    _ => None,
  }
}

async fn serve_custom_css(State(state): State<AppState>) -> Response {
  let Some(path) = state.config.ui.custom_css.as_ref() else {
    return StatusCode::NOT_FOUND.into_response();
  };

  match tokio::fs::read_to_string(path).await {
    Ok(css) =>
    {
      #[expect(
        clippy::expect_used,
        reason = "response builder with static values cannot fail"
      )]
      {
        Response::builder()
          .header(header::CONTENT_TYPE, "text/css")
          .header(header::CACHE_CONTROL, "no-cache")
          .body(Body::from(css))
          .expect("response builder should not fail")
      }
      .into_response()
    },
    Err(error) => {
      tracing::warn!(path = %path.display(), "failed to read custom CSS: {error}");
      StatusCode::NOT_FOUND.into_response()
    },
  }
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

pub fn public_router(config: &Config) -> Router<AppState> {
  let mut router = Router::new()
    .merge(health::router())
    .merge(badges::router())
    .merge(cache::router())
    .merge(channel_manifests::router())
    .merge(metrics::router())
    // Webhooks use their own HMAC auth, outside the API key gate.
    .merge(webhooks::router())
    // OAuth and LDAP routes use their own auth mechanisms.
    .merge(oauth::router())
    .merge(ldap::router());

  if config.server.openapi_enabled {
    router = router.merge(openapi::router());
  }

  router
}

pub fn ui_router(state: AppState, config: &Config) -> Router<AppState> {
  let mut router = Router::new();

  if config.ui.assets_enabled() {
    router = router
      .route("/static/style.css", get(serve_style_css))
      .route("/static/theme.css", get(serve_theme_css));

    if config.ui.custom_css.is_some() {
      router = router.route("/static/custom.css", get(serve_custom_css));
    }

    if let Some(static_dir) = config.ui.static_dir.as_ref() {
      router = router.nest_service("/static/custom", ServeDir::new(static_dir));
    }
  }

  if config.ui.dashboard_enabled() {
    router = router.merge(
      dashboard::router()
        .route_layer(middleware::from_fn_with_state(state, extract_session)),
    );
  }

  router
}

pub fn router(state: AppState, config: &Config) -> Router {
  let server_config = &config.server;
  let cors_layer = if server_config.cors_permissive {
    tracing::warn!(
      "server.cors_permissive is enabled; CORS credentials are disabled"
    );
    CorsLayer::new()
      .allow_origin(Any)
      .allow_methods(Any)
      .allow_headers(Any)
      .allow_credentials(false)
  } else if server_config.allowed_origins.is_empty() {
    CorsLayer::new()
  } else {
    let origins: Vec<HeaderValue> = server_config
      .allowed_origins
      .iter()
      .filter_map(|o| o.parse().ok())
      .collect();
    CorsLayer::new().allow_origin(AllowOrigin::list(origins))
  };

  let mut app = Router::new()
    .nest("/api/v1", api_router(state.clone()))
    .merge(public_router(config));

  if config.ui.enabled {
    app = app.merge(ui_router(state.clone(), config));
  }

  app = app
        .layer(TraceLayer::new_for_http())
        .layer(cors_layer)
        .layer(RequestBodyLimitLayer::new(server_config.max_body_size))
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
    (server_config.rate_limit_rps, server_config.rate_limit_burst)
  {
    let rl_state = Arc::new(RateLimitState {
      buckets:      DashMap::new(),
      rps:          rps as f64,
      burst:        f64::from(burst),
      last_cleanup: Mutex::new(Instant::now()),
    });
    app = app
      .layer(axum::Extension(rl_state))
      .layer(middleware::from_fn(rate_limit_middleware));
  }

  app.with_state(state)
}

#[cfg(test)]
mod tests {
  use super::accent_contrast_color;

  #[test]
  fn accent_contrast_chooses_readable_text_for_hex_colors() {
    assert_eq!(accent_contrast_color("#111827"), Some("#ffffff"));
    assert_eq!(accent_contrast_color("#f4f6f8"), Some("#0b0f14"));
    assert_eq!(accent_contrast_color("#000"), Some("#ffffff"));
    assert_eq!(accent_contrast_color("#fff"), Some("#0b0f14"));
    assert_eq!(accent_contrast_color("currentColor"), None);
  }
}
