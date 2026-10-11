//! OAuth authentication routes

use axum::{
  Router,
  extract::{Query, State},
  http::{HeaderMap, StatusCode, header},
  response::{IntoResponse, Response},
  routing::get,
};
use circus_common::{models::UserType, repo};
use circus_config::GitHubOAuthConfig;
use circus_http::{RequestExt as _, ResponseExt as _};
use circus_logs::ErrorChain;
use serde::Deserialize;
use subtle::ConstantTimeEq;
use url::Url;

use super::super::{error::ApiError, state::AppState};
use crate::{
  oauth_client::{AuthCodeClient, ClientAuth, Secret},
  session_cookie::{
    OAUTH_STATE_COOKIE,
    USER_SESSION_MAX_AGE_SECS,
    clear_oauth_state_cookie,
    cookie_value,
    oauth_state_cookie,
    oauth_user_session_cookie,
  },
};

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackParams {
  code:  String,
  state: String,
}

#[derive(Deserialize)]
struct GitHubTokenResponse {
  access_token: String,
}

#[derive(Debug, Deserialize)]
struct GitHubUserResponse {
  id:         i64,
  login:      String,
  avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubEmailResponse {
  email:    String,
  primary:  bool,
  verified: bool,
}

#[expect(
  clippy::expect_used,
  reason = "hard-coded GitHub endpoints are valid URLs"
)]
fn build_github_client(config: &GitHubOAuthConfig) -> AuthCodeClient {
  AuthCodeClient {
    client_id:     config.client_id.clone(),
    client_secret: Some(config.client_secret.clone()),
    auth_url:      Url::parse("https://github.com/login/oauth/authorize")
      .expect("valid auth url"),
    token_url:     Url::parse("https://github.com/login/oauth/access_token")
      .expect("valid token url"),
    redirect_uri:  config.redirect_uri.clone(),
    auth:          ClientAuth::Basic,
  }
}

async fn github_login(State(state): State<AppState>) -> impl IntoResponse {
  let Some(config) = &state.config.oauth.github else {
    return (StatusCode::NOT_FOUND, "GitHub OAuth not configured")
      .into_response();
  };

  let Ok(csrf_token) = Secret::random() else {
    return (StatusCode::INTERNAL_SERVER_ERROR, "No randomness available")
      .into_response();
  };
  let auth_url = build_github_client(config).authorize_url(
    &csrf_token,
    ["read:user", "user:email"],
    &[],
  );

  let cookie = oauth_state_cookie(
    csrf_token.as_str(),
    &state.config.server,
    &config.redirect_uri,
  );

  #[expect(
    clippy::expect_used,
    reason = "response builder with static values cannot fail"
  )]
  {
    Response::builder()
      .status(StatusCode::FOUND)
      .header(header::LOCATION, auth_url.as_str())
      .header(header::SET_COOKIE, cookie)
      .body(axum::body::Body::empty())
      .expect("response builder should not fail")
  }
  .into_response()
}

async fn github_callback(
  State(state): State<AppState>,
  headers: HeaderMap,
  Query(params): Query<OAuthCallbackParams>,
) -> Result<impl IntoResponse, ApiError> {
  let Some(config) = &state.config.oauth.github else {
    return Err(ApiError(circus_common::CiError::NotFound(
      "GitHub OAuth not configured".to_string(),
    )));
  };

  // Verify CSRF token from cookie. Use constant-time comparison: the
  // received state and the cookie value are both attacker-controlled
  // inputs to the compare, and a timing leak would reveal a valid token.
  let stored_state = cookie_value(&headers, OAUTH_STATE_COOKIE);

  let state_ok = stored_state.is_some_and(|s| {
    s.len() == params.state.len()
      && s.as_bytes().ct_eq(params.state.as_bytes()).into()
  });
  if !state_ok {
    return Err(ApiError(circus_common::CiError::Unauthorized(
      "Invalid OAuth state".to_string(),
    )));
  }

  let token: GitHubTokenResponse = build_github_client(config)
    .exchange(&state.http_client, &params.code, None)
    .await
    .map_err(|e| {
      ApiError(circus_common::CiError::Internal(format!(
        "Token exchange failed: {}",
        ErrorChain(&e)
      )))
    })?;
  let access_token = token.access_token;

  let github_api = |path: &str| {
    state
      .http_client
      .get(format!("https://api.github.com{path}"))
      .bearer_auth(&access_token)
      .header("Accept", "application/vnd.github+json")
      .header("X-GitHub-Api-Version", circus_http::GITHUB_API_VERSION)
  };
  let internal = |e: circus_http::Error| {
    ApiError(circus_common::CiError::Internal(format!(
      "GitHub API request failed: {}",
      ErrorChain(&e)
    )))
  };

  let user_info: GitHubUserResponse = github_api("/user")
    .send_checked()
    .await
    .map_err(internal)?
    .json_capped(circus_http::JSON_LIMIT)
    .await
    .map_err(internal)?;
  tracing::debug!(
    github_id = user_info.id,
    github_login = %user_info.login,
    avatar_url = user_info.avatar_url.as_deref(),
    "GitHub OAuth user profile"
  );

  let emails: Vec<GitHubEmailResponse> = github_api("/user/emails")
    .send_checked()
    .await
    .map_err(internal)?
    .json_capped(circus_http::JSON_LIMIT)
    .await
    .map_err(internal)?;

  let primary_email = emails
    .iter()
    .find(|e| e.primary && e.verified)
    .or_else(|| emails.iter().find(|e| e.verified))
    .map(|e| e.email.clone());

  // Create or update user in database
  let user = repo::users::upsert_oauth_user(
    &state.pool,
    &user_info.login,
    primary_email.as_deref(),
    UserType::Github,
    &user_info.id.to_string(),
    state.email_regex.as_deref(),
  )
  .await?;

  // Create session
  let session = repo::users::create_session(&state.pool, user.id).await?;

  let clear_state =
    clear_oauth_state_cookie(&state.config.server, &config.redirect_uri);
  let session_cookie = oauth_user_session_cookie(
    &session.0,
    &state.config.server,
    &config.redirect_uri,
    USER_SESSION_MAX_AGE_SECS,
  );

  Ok(
    #[expect(
      clippy::expect_used,
      reason = "response builder with static values cannot fail"
    )]
    {
      Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, "/")
        .header(header::SET_COOKIE, clear_state)
        .header(header::SET_COOKIE, session_cookie)
        .body(axum::body::Body::empty())
        .expect("response builder should not fail")
    },
  )
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/api/v1/auth/github", get(github_login))
    .route("/api/v1/auth/github/callback", get(github_callback))
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "Fine in tests")]
mod tests {
  use super::*;

  #[test]
  fn test_build_github_client() {
    let config = GitHubOAuthConfig {
      client_id:          "test_client_id".to_string(),
      client_secret:      "test_client_secret".to_string(),
      client_secret_file: None,
      redirect_uri:       "http://localhost:3000/api/v1/auth/github/callback"
        .to_string(),
    };

    // Should not panic
    let _client = build_github_client(&config);
  }

  #[test]
  fn test_build_github_client_https() {
    let config = GitHubOAuthConfig {
      client_id:          "test_client_id".to_string(),
      client_secret:      "test_client_secret".to_string(),
      client_secret_file: None,
      redirect_uri:       "https://example.com/api/v1/auth/github/callback"
        .to_string(),
    };

    // Should not panic with HTTPS redirect URI
    let _client = build_github_client(&config);
  }

  #[test]
  fn test_authorize_url_generation() {
    let config = GitHubOAuthConfig {
      client_id:          "test_client_id".to_string(),
      client_secret:      "test_client_secret".to_string(),
      client_secret_file: None,
      redirect_uri:       "http://localhost:3000/api/v1/auth/github/callback"
        .to_string(),
    };

    let csrf_token = Secret::random().expect("randomness");
    let auth_url = build_github_client(&config).authorize_url(
      &csrf_token,
      ["read:user"],
      &[],
    );

    let url_str = auth_url.as_str();
    assert!(url_str.starts_with("https://github.com/login/oauth/authorize"));
    assert!(url_str.contains("client_id=test_client_id"));
    assert!(url_str.contains("scope=read%3Auser"));
    assert!(url_str.contains(&format!("state={}", csrf_token.as_str())));
  }

  #[test]
  fn test_secure_flag_detection() {
    // HTTP should not have Secure flag
    let http_uri = "http://localhost:3000/callback";
    let http_secure_flag = if http_uri.starts_with("https://") {
      "; Secure"
    } else {
      ""
    };
    assert_eq!(http_secure_flag, "");

    // HTTPS should have Secure flag
    let https_uri = "https://example.com/callback";
    let https_secure_flag = if https_uri.starts_with("https://") {
      "; Secure"
    } else {
      ""
    };
    assert_eq!(https_secure_flag, "; Secure");
  }

  #[test]
  fn test_oauth_callback_params_deserialize() {
    let json = r#"{"code": "abc123", "state": "xyz789"}"#;
    let params: OAuthCallbackParams = serde_json::from_str(json).unwrap();
    assert_eq!(params.code, "abc123");
    assert_eq!(params.state, "xyz789");
  }

  #[test]
  fn test_github_user_response_deserialize() {
    let json = r#"{
      "id": 12345,
      "login": "testuser",
      "avatar_url": "https://avatars.githubusercontent.com/u/12345"
    }"#;
    let user: GitHubUserResponse = serde_json::from_str(json).unwrap();
    assert_eq!(user.id, 12345);
    assert_eq!(user.login, "testuser");
    assert_eq!(
      user.avatar_url,
      Some("https://avatars.githubusercontent.com/u/12345".to_string())
    );
  }

  #[test]
  fn test_github_user_response_minimal() {
    // avatar_url is optional
    let json = r#"{"id": 12345, "login": "testuser", "avatar_url": null}"#;
    let user: GitHubUserResponse = serde_json::from_str(json).unwrap();
    assert_eq!(user.id, 12345);
    assert_eq!(user.login, "testuser");
    assert!(user.avatar_url.is_none());
  }

  #[test]
  fn test_github_email_response_deserialize() {
    let json = r#"{
      "email": "user@example.com",
      "primary": true,
      "verified": true
    }"#;
    let email: GitHubEmailResponse = serde_json::from_str(json).unwrap();
    assert_eq!(email.email, "user@example.com");
    assert!(email.primary);
    assert!(email.verified);
  }

  #[test]
  fn test_github_emails_find_primary_verified() {
    let emails = [
      GitHubEmailResponse {
        email:    "secondary@example.com".to_string(),
        primary:  false,
        verified: true,
      },
      GitHubEmailResponse {
        email:    "primary@example.com".to_string(),
        primary:  true,
        verified: true,
      },
      GitHubEmailResponse {
        email:    "unverified@example.com".to_string(),
        primary:  false,
        verified: false,
      },
    ];

    let primary_email = emails
      .iter()
      .find(|e| e.primary && e.verified)
      .or_else(|| emails.iter().find(|e| e.verified))
      .map(|e| e.email.clone());

    assert_eq!(primary_email, Some("primary@example.com".to_string()));
  }

  #[test]
  fn test_github_emails_fallback_to_verified() {
    // No primary email, should fall back to first verified
    let emails = [
      GitHubEmailResponse {
        email:    "unverified@example.com".to_string(),
        primary:  false,
        verified: false,
      },
      GitHubEmailResponse {
        email:    "verified@example.com".to_string(),
        primary:  false,
        verified: true,
      },
    ];

    let primary_email = emails
      .iter()
      .find(|e| e.primary && e.verified)
      .or_else(|| emails.iter().find(|e| e.verified))
      .map(|e| e.email.clone());

    assert_eq!(primary_email, Some("verified@example.com".to_string()));
  }

  #[test]
  fn test_github_emails_no_verified() {
    // No verified emails
    let emails = [GitHubEmailResponse {
      email:    "unverified@example.com".to_string(),
      primary:  true,
      verified: false,
    }];

    let primary_email = emails
      .iter()
      .find(|e| e.primary && e.verified)
      .or_else(|| emails.iter().find(|e| e.verified))
      .map(|e| e.email.clone());

    assert!(primary_email.is_none());
  }

  #[test]
  fn test_cookie_parsing() {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
      axum::http::header::COOKIE,
      "other_cookie=value; circus_oauth_state=abc123; another=xyz"
        .parse()
        .unwrap(),
    );

    assert_eq!(
      cookie_value(&headers, OAUTH_STATE_COOKIE).as_deref(),
      Some("abc123")
    );
  }

  #[test]
  fn test_cookie_parsing_not_found() {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
      axum::http::header::COOKIE,
      "other_cookie=value; another=xyz".parse().unwrap(),
    );

    assert!(cookie_value(&headers, OAUTH_STATE_COOKIE).is_none());
  }

  #[test]
  fn test_session_cookie_format() {
    let session_token = "test-session-token";
    let config = circus_config::ServerConfig {
      force_secure_cookies: true,
      ..Default::default()
    };
    let cookie = oauth_user_session_cookie(
      session_token,
      &config,
      "https://example.com/callback",
      USER_SESSION_MAX_AGE_SECS,
    );

    assert!(cookie.contains("circus_user_session=test-session-token"));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));
    assert!(cookie.contains("Path=/"));
    assert!(cookie.contains("Max-Age=604800")); // 7 days in seconds
    assert!(cookie.contains("Secure"));
  }
}
