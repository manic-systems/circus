//! OAuth authentication routes

use axum::{
  Router,
  extract::{Query, State},
  http::{StatusCode, header},
  response::{IntoResponse, Response},
  routing::get,
};
use axum_extra::extract::cookie::CookieJar;
use circus_common::{models::UserType, repo};
use circus_config::GitHubOAuthConfig;
use oauth2::{
  AuthUrl,
  AuthorizationCode,
  ClientId,
  ClientSecret,
  CsrfToken,
  EndpointNotSet,
  EndpointSet,
  RedirectUrl,
  Scope,
  StandardErrorResponse,
  StandardRevocableToken,
  StandardTokenIntrospectionResponse,
  StandardTokenResponse,
  TokenResponse,
  TokenUrl,
  basic::{BasicClient, BasicErrorResponseType, BasicTokenType},
};
use serde::Deserialize;
use subtle::ConstantTimeEq;

use super::super::{error::ApiError, state::AppState};
use crate::session_cookie::{
  OAUTH_STATE_COOKIE,
  clear_oauth_state_cookie,
  oauth_state_cookie,
  oauth_user_session_cookie,
};

/// Type alias for the fully-configured GitHub OAuth client (oauth2 v5.0
/// type-state)
type GitHubOAuthClient = oauth2::Client<
  StandardErrorResponse<BasicErrorResponseType>,
  StandardTokenResponse<oauth2::EmptyExtraTokenFields, BasicTokenType>,
  StandardTokenIntrospectionResponse<
    oauth2::EmptyExtraTokenFields,
    BasicTokenType,
  >,
  StandardRevocableToken,
  StandardErrorResponse<oauth2::RevocationErrorResponseType>,
  EndpointSet,
  EndpointNotSet,
  EndpointNotSet,
  EndpointNotSet,
  EndpointSet,
>;

#[derive(Debug, Deserialize)]
pub struct OAuthCallbackParams {
  code:  String,
  state: String,
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
  reason = "hard-coded URLs and validated redirect URI are infallible"
)]
fn build_github_client(config: &GitHubOAuthConfig) -> GitHubOAuthClient {
  let auth_url =
    AuthUrl::new("https://github.com/login/oauth/authorize".to_string())
      .expect("valid auth url");
  let token_url =
    TokenUrl::new("https://github.com/login/oauth/access_token".to_string())
      .expect("valid token url");

  // oauth2 v5.0 uses builder pattern with type-state
  BasicClient::new(ClientId::new(config.client_id.clone()))
    .set_client_secret(ClientSecret::new(config.client_secret.clone()))
    .set_auth_uri(auth_url)
    .set_token_uri(token_url)
    .set_redirect_uri(
      RedirectUrl::new(config.redirect_uri.clone())
        .expect("valid redirect url"),
    )
}

async fn github_login(State(state): State<AppState>) -> impl IntoResponse {
  let Some(config) = &state.config.oauth.github else {
    return (StatusCode::NOT_FOUND, "GitHub OAuth not configured")
      .into_response();
  };

  let client = build_github_client(config);
  let (auth_url, csrf_token) = client
    .authorize_url(CsrfToken::new_random)
    .add_scope(Scope::new("read:user".to_string()))
    .add_scope(Scope::new("user:email".to_string()))
    .url();

  let cookie = oauth_state_cookie(
    csrf_token.secret(),
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
  jar: CookieJar,
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
  let stored_state = jar
    .get(OAUTH_STATE_COOKIE)
    .map(axum_extra::extract::cookie::Cookie::value);

  let state_ok = stored_state.is_some_and(|s| {
    s.len() == params.state.len()
      && s.as_bytes().ct_eq(params.state.as_bytes()).into()
  });
  if !state_ok {
    return Err(ApiError(circus_common::CiError::Unauthorized(
      "Invalid OAuth state".to_string(),
    )));
  }

  let client = build_github_client(config);

  // Create HTTP client for oauth2 v5.0 token exchange
  let http_client = oauth2::reqwest::ClientBuilder::new()
    .redirect(oauth2::reqwest::redirect::Policy::none())
    .build()
    .map_err(|e| {
      ApiError(circus_common::CiError::Internal(format!(
        "Failed to create HTTP client: {e}"
      )))
    })?;

  // Exchange code for access token
  let token_result = client
    .exchange_code(AuthorizationCode::new(params.code))
    .request_async(&http_client)
    .await
    .map_err(|e| {
      ApiError(circus_common::CiError::Internal(format!(
        "Token exchange failed: {e}"
      )))
    })?;

  let access_token = token_result.access_token().secret();

  // Fetch user info from GitHub using shared HTTP client
  let user_response = state
    .http_client
    .get("https://api.github.com/user")
    .header("Authorization", format!("Bearer {access_token}"))
    .header("User-Agent", "circus")
    .header("Accept", "application/vnd.github+json")
    .send()
    .await
    .map_err(|e| {
      ApiError(circus_common::CiError::Internal(format!(
        "GitHub API request failed: {e}"
      )))
    })?;

  if !user_response.status().is_success() {
    return Err(ApiError(circus_common::CiError::Internal(format!(
      "GitHub API returned status: {}",
      user_response.status()
    ))));
  }

  let user_info: GitHubUserResponse =
    user_response.json().await.map_err(|e| {
      ApiError(circus_common::CiError::Internal(format!(
        "Failed to parse GitHub user: {e}"
      )))
    })?;
  tracing::debug!(
    github_id = user_info.id,
    github_login = %user_info.login,
    avatar_url = user_info.avatar_url.as_deref(),
    "GitHub OAuth user profile"
  );

  // Fetch user emails
  let emails_response = state
    .http_client
    .get("https://api.github.com/user/emails")
    .header("Authorization", format!("Bearer {access_token}"))
    .header("User-Agent", "circus")
    .header("Accept", "application/vnd.github+json")
    .send()
    .await
    .map_err(|e| {
      ApiError(circus_common::CiError::Internal(format!(
        "GitHub emails API failed: {e}"
      )))
    })?;

  if !emails_response.status().is_success() {
    return Err(ApiError(circus_common::CiError::Internal(format!(
      "GitHub emails API returned status: {}",
      emails_response.status()
    ))));
  }

  let emails: Vec<GitHubEmailResponse> =
    emails_response.json().await.map_err(|e| {
      ApiError(circus_common::CiError::Internal(format!(
        "Failed to parse GitHub emails: {e}"
      )))
    })?;

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

    let client = build_github_client(&config);
    let (auth_url, csrf_token) = client
      .authorize_url(CsrfToken::new_random)
      .add_scope(Scope::new("read:user".to_string()))
      .url();

    let url_str = auth_url.as_str();
    assert!(url_str.starts_with("https://github.com/login/oauth/authorize"));
    assert!(url_str.contains("client_id=test_client_id"));
    assert!(url_str.contains("scope=read%3Auser"));
    assert!(!csrf_token.secret().is_empty());
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

    let jar = CookieJar::from_headers(&headers);
    let stored_state = jar
      .get(OAUTH_STATE_COOKIE)
      .map(axum_extra::extract::cookie::Cookie::value);

    assert_eq!(stored_state, Some("abc123"));
  }

  #[test]
  fn test_cookie_parsing_not_found() {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
      axum::http::header::COOKIE,
      "other_cookie=value; another=xyz".parse().unwrap(),
    );

    let jar = CookieJar::from_headers(&headers);
    assert!(jar.get(OAUTH_STATE_COOKIE).is_none());
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
    );

    assert!(cookie.contains("circus_user_session=test-session-token"));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));
    assert!(cookie.contains("Path=/"));
    assert!(cookie.contains("Max-Age=604800")); // 7 days in seconds
    assert!(cookie.contains("Secure"));
  }
}
