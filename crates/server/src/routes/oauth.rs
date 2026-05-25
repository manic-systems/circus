//! OAuth authentication routes

use axum::{
  Router,
  extract::{Query, State},
  http::{StatusCode, header},
  response::{IntoResponse, Response},
  routing::get,
};
use circus_common::{config::GitHubOAuthConfig, models::UserType, repo};
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

use crate::{error::ApiError, state::AppState};

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
  #[allow(dead_code)]
  avatar_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GitHubEmailResponse {
  email:    String,
  primary:  bool,
  verified: bool,
}

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

  // Store CSRF token in a cookie for verification
  // Use SameSite=Lax for OAuth flow (must work across redirect)
  let security_flags = {
    let is_localhost = config.redirect_uri.starts_with("http://localhost")
      || config.redirect_uri.starts_with("http://127.0.0.1");

    let secure_flag = if state.config.server.force_secure_cookies
      || (!is_localhost && config.redirect_uri.starts_with("https://"))
    {
      "; Secure"
    } else {
      ""
    };

    format!("HttpOnly; SameSite=Lax{secure_flag}")
  };

  let cookie = format!(
    "circus_oauth_state={}; {}; Path=/; Max-Age=600",
    csrf_token.secret(),
    security_flags
  );

  Response::builder()
    .status(StatusCode::FOUND)
    .header(header::LOCATION, auth_url.as_str())
    .header(header::SET_COOKIE, cookie)
    .body(axum::body::Body::empty())
    .unwrap()
    .into_response()
}

async fn github_callback(
  State(state): State<AppState>,
  headers: axum::http::HeaderMap,
  Query(params): Query<OAuthCallbackParams>,
) -> Result<impl IntoResponse, ApiError> {
  let Some(config) = &state.config.oauth.github else {
    return Err(ApiError(circus_common::CiError::NotFound(
      "GitHub OAuth not configured".to_string(),
    )));
  };

  // Verify CSRF token from cookie
  let stored_state = headers
    .get(header::COOKIE)
    .and_then(|c| c.to_str().ok())
    .and_then(|cookies| {
      cookies.split(';').find_map(|c| {
        let c = c.trim();
        c.strip_prefix("circus_oauth_state=")
      })
    });

  if stored_state != Some(&params.state) {
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
  )
  .await
  .map_err(ApiError)?;

  // Create session
  let session = repo::users::create_session(&state.pool, user.id)
    .await
    .map_err(ApiError)?;

  // Clear OAuth state cookie and set session cookie
  // Use SameSite=Lax for OAuth callback (must work across redirect)
  let security_flags = {
    let is_localhost = config.redirect_uri.starts_with("http://localhost")
      || config.redirect_uri.starts_with("http://127.0.0.1");

    let secure_flag = if state.config.server.force_secure_cookies
      || (!is_localhost && config.redirect_uri.starts_with("https://"))
    {
      "; Secure"
    } else {
      ""
    };

    format!("HttpOnly; SameSite=Lax{secure_flag}")
  };

  let clear_state =
    format!("circus_oauth_state=; {security_flags}; Path=/; Max-Age=0");
  let session_cookie = format!(
    "circus_user_session={}; {}; Path=/; Max-Age={}",
    session.0,
    security_flags,
    7 * 24 * 60 * 60 // 7 days
  );

  Ok(
    Response::builder()
      .status(StatusCode::FOUND)
      .header(header::LOCATION, "/")
      .header(header::SET_COOKIE, clear_state)
      .header(header::SET_COOKIE, session_cookie)
      .body(axum::body::Body::empty())
      .unwrap(),
  )
}

pub fn router() -> Router<AppState> {
  Router::new()
    .route("/api/v1/auth/github", get(github_login))
    .route("/api/v1/auth/github/callback", get(github_callback))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_build_github_client() {
    let config = GitHubOAuthConfig {
      client_id:     "test_client_id".to_string(),
      client_secret: "test_client_secret".to_string(),
      redirect_uri:  "http://localhost:3000/api/v1/auth/github/callback"
        .to_string(),
    };

    // Should not panic
    let _client = build_github_client(&config);
  }

  #[test]
  fn test_build_github_client_https() {
    let config = GitHubOAuthConfig {
      client_id:     "test_client_id".to_string(),
      client_secret: "test_client_secret".to_string(),
      redirect_uri:  "https://example.com/api/v1/auth/github/callback"
        .to_string(),
    };

    // Should not panic with HTTPS redirect URI
    let _client = build_github_client(&config);
  }

  #[test]
  fn test_authorize_url_generation() {
    let config = GitHubOAuthConfig {
      client_id:     "test_client_id".to_string(),
      client_secret: "test_client_secret".to_string(),
      redirect_uri:  "http://localhost:3000/api/v1/auth/github/callback"
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
    // Simulate parsing cookies to find OAuth state
    let cookie_header =
      "other_cookie=value; circus_oauth_state=abc123; another=xyz";

    let stored_state = cookie_header.split(';').find_map(|c| {
      let c = c.trim();
      c.strip_prefix("circus_oauth_state=")
    });

    assert_eq!(stored_state, Some("abc123"));
  }

  #[test]
  fn test_cookie_parsing_not_found() {
    let cookie_header = "other_cookie=value; another=xyz";

    let stored_state = cookie_header.split(';').find_map(|c| {
      let c = c.trim();
      c.strip_prefix("circus_oauth_state=")
    });

    assert!(stored_state.is_none());
  }

  #[test]
  fn test_session_cookie_format() {
    let session_token = "test-session-token";
    let secure_flag = "; Secure";
    let max_age = 7 * 24 * 60 * 60;

    let cookie = format!(
      "circus_user_session={session_token}; HttpOnly; SameSite=Lax; Path=/; \
       Max-Age={max_age}{secure_flag}"
    );

    assert!(cookie.contains("circus_user_session=test-session-token"));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Lax"));
    assert!(cookie.contains("Path=/"));
    assert!(cookie.contains("Max-Age=604800")); // 7 days in seconds
    assert!(cookie.contains("Secure"));
  }
}
