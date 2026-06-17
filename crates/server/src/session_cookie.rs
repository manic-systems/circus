use axum::http::HeaderMap;
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use circus_config::ServerConfig;
use time::Duration as CookieDuration;

pub const USER_SESSION_COOKIE: &str = "circus_user_session";
pub const API_KEY_SESSION_COOKIE: &str = "circus_session";
pub const OAUTH_STATE_COOKIE: &str = "circus_oauth_state";

pub const USER_SESSION_MAX_AGE_SECS: i64 = 7 * 24 * 60 * 60;
pub const API_KEY_SESSION_MAX_AGE_SECS: i64 = 24 * 60 * 60;
pub const OAUTH_STATE_MAX_AGE_SECS: i64 = 10 * 60;

pub const API_KEY_SESSION_MAX_AGE: std::time::Duration =
  std::time::Duration::from_secs(API_KEY_SESSION_MAX_AGE_SECS as u64);

#[must_use]
pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
  CookieJar::from_headers(headers)
    .get(name)
    .map(|cookie| cookie.value().to_string())
}

#[must_use]
pub fn user_session_cookie(token: &str, config: &ServerConfig) -> String {
  persistent_cookie(
    USER_SESSION_COOKIE,
    token,
    USER_SESSION_MAX_AGE_SECS,
    SameSite::Strict,
    server_cookie_secure(config),
  )
}

#[must_use]
pub fn api_key_session_cookie(token: &str, config: &ServerConfig) -> String {
  persistent_cookie(
    API_KEY_SESSION_COOKIE,
    token,
    API_KEY_SESSION_MAX_AGE_SECS,
    SameSite::Strict,
    server_cookie_secure(config),
  )
}

#[must_use]
pub fn oauth_state_cookie(
  token: &str,
  config: &ServerConfig,
  redirect_uri: &str,
) -> String {
  persistent_cookie(
    OAUTH_STATE_COOKIE,
    token,
    OAUTH_STATE_MAX_AGE_SECS,
    SameSite::Lax,
    oauth_cookie_secure(config, redirect_uri),
  )
}

#[must_use]
pub fn oauth_user_session_cookie(
  token: &str,
  config: &ServerConfig,
  redirect_uri: &str,
) -> String {
  persistent_cookie(
    USER_SESSION_COOKIE,
    token,
    USER_SESSION_MAX_AGE_SECS,
    SameSite::Lax,
    oauth_cookie_secure(config, redirect_uri),
  )
}

#[must_use]
pub fn clear_cookie(name: &'static str, config: &ServerConfig) -> String {
  cookie(name, "", 0, SameSite::Strict, server_cookie_secure(config))
}

#[must_use]
pub fn clear_oauth_state_cookie(
  config: &ServerConfig,
  redirect_uri: &str,
) -> String {
  cookie(
    OAUTH_STATE_COOKIE,
    "",
    0,
    SameSite::Lax,
    oauth_cookie_secure(config, redirect_uri),
  )
}

fn persistent_cookie(
  name: &'static str,
  value: &str,
  max_age_secs: i64,
  same_site: SameSite,
  secure: bool,
) -> String {
  cookie(name, value, max_age_secs, same_site, secure)
}

fn cookie(
  name: &'static str,
  value: &str,
  max_age_secs: i64,
  same_site: SameSite,
  secure: bool,
) -> String {
  Cookie::build((name, value.to_string()))
    .http_only(true)
    .same_site(same_site)
    .secure(secure)
    .path("/")
    .max_age(CookieDuration::seconds(max_age_secs))
    .build()
    .to_string()
}

fn server_cookie_secure(config: &ServerConfig) -> bool {
  config.force_secure_cookies
    || !matches!(config.host.as_str(), "127.0.0.1" | "localhost" | "::1")
}

fn oauth_cookie_secure(config: &ServerConfig, redirect_uri: &str) -> bool {
  let is_localhost = redirect_uri.starts_with("http://localhost")
    || redirect_uri.starts_with("http://127.0.0.1");
  config.force_secure_cookies
    || (!is_localhost && redirect_uri.starts_with("https://"))
}
