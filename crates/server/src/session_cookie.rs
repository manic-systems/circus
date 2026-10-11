use axum::http::{HeaderMap, header::COOKIE};
use circus_config::ServerConfig;
use uuid::Uuid;

pub const USER_SESSION_COOKIE: &str = "circus_user_session";
pub const API_KEY_SESSION_COOKIE: &str = "circus_session";
pub const OAUTH_STATE_COOKIE: &str = "circus_oauth_state";
pub const OIDC_FLOW_COOKIE: &str = "circus_oidc_flow";
/// Names the provider a session signed in with, so logout knows where to end
/// the provider session.
pub const OIDC_PROVIDER_COOKIE: &str = "circus_oidc_provider";

pub const USER_SESSION_MAX_AGE_SECS: i64 = 7 * 24 * 60 * 60;
pub const API_KEY_SESSION_MAX_AGE_SECS: i64 = 24 * 60 * 60;
pub const OAUTH_STATE_MAX_AGE_SECS: i64 = 10 * 60;

pub const API_KEY_SESSION_MAX_AGE: std::time::Duration =
  std::time::Duration::from_secs(API_KEY_SESSION_MAX_AGE_SECS as u64);

#[derive(Clone, Copy)]
enum SameSite {
  Strict,
  Lax,
}

/// The value of the first `name` cookie across every `Cookie` header.
#[must_use]
pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
  headers
    .get_all(COOKIE)
    .iter()
    .filter_map(|header| header.to_str().ok())
    .flat_map(|header| header.split(';'))
    .filter_map(|pair| pair.trim().split_once('='))
    .find(|(key, _)| *key == name)
    .map(|(_, value)| value.to_owned())
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
  max_age_secs: i64,
) -> String {
  persistent_cookie(
    USER_SESSION_COOKIE,
    token,
    max_age_secs,
    SameSite::Lax,
    oauth_cookie_secure(config, redirect_uri),
  )
}

/// Carries a new API key's secret across the redirect to `/admin`, the one
/// page that shows it. Named per key so concurrent creations keep their own.
#[must_use]
pub fn new_api_key_cookie_name(key_id: Uuid) -> String {
  format!("circus_new_api_key_{}", key_id.simple())
}

/// `max_age_secs` of 0 clears it.
#[must_use]
pub fn new_api_key_cookie(
  key_id: Uuid,
  key: &str,
  max_age_secs: i64,
  config: &ServerConfig,
) -> String {
  set_cookie(
    &new_api_key_cookie_name(key_id),
    key,
    "/admin",
    max_age_secs,
    SameSite::Strict,
    server_cookie_secure(config),
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

#[must_use]
pub fn oidc_flow_cookie(
  value: &str,
  config: &ServerConfig,
  redirect_uri: &str,
) -> String {
  set_cookie(
    OIDC_FLOW_COOKIE,
    value,
    "/api/v1/auth/oidc/",
    OAUTH_STATE_MAX_AGE_SECS,
    SameSite::Lax,
    oauth_cookie_secure(config, redirect_uri),
  )
}

#[must_use]
pub fn clear_oidc_flow_cookie(
  config: &ServerConfig,
  redirect_uri: &str,
) -> String {
  set_cookie(
    OIDC_FLOW_COOKIE,
    "",
    "/api/v1/auth/oidc/",
    0,
    SameSite::Lax,
    oauth_cookie_secure(config, redirect_uri),
  )
}

#[must_use]
pub fn oidc_provider_cookie(
  provider: &str,
  config: &ServerConfig,
  redirect_uri: &str,
  max_age_secs: i64,
) -> String {
  persistent_cookie(
    OIDC_PROVIDER_COOKIE,
    provider,
    max_age_secs,
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
  set_cookie(name, value, "/", max_age_secs, same_site, secure)
}

fn set_cookie(
  name: &str,
  value: &str,
  path: &str,
  max_age_secs: i64,
  same_site: SameSite,
  secure: bool,
) -> String {
  let same_site = match same_site {
    SameSite::Strict => "Strict",
    SameSite::Lax => "Lax",
  };
  let secure = if secure { "; Secure" } else { "" };
  format!(
    "{name}={value}; HttpOnly; SameSite={same_site}{secure}; Path={path}; \
     Max-Age={max_age_secs}"
  )
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
