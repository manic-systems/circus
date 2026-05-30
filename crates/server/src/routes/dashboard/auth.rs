//! Dashboard login/logout handlers.
//!
//! Login accepts either user credentials (username + password) or a raw
//! API key. Successful logins set a session cookie with the configured
//! `Secure` flag policy. Logout drops both legacy cookie names so users
//! coming off an older session are fully cleaned up.

use askama::Template;
use axum::{
  Form,
  extract::State,
  http::StatusCode,
  response::{Html, IntoResponse, Redirect, Response},
};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::templates::LoginTemplate;
use crate::state::AppState;

pub(super) async fn login_page() -> Html<String> {
  let tmpl = LoginTemplate { error: None };
  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
}

#[derive(serde::Deserialize)]
pub(super) struct LoginForm {
  username: Option<String>,
  api_key:  Option<String>,
  password: Option<String>,
}

pub(super) async fn login_action(
  State(state): State<AppState>,
  Form(form): Form<LoginForm>,
) -> Response {
  // Try username/password authentication first
  if let (Some(username), Some(password)) =
    (form.username.as_ref(), form.password.as_ref())
  {
    let creds = circus_common::models::LoginCredentials {
      username: username.clone(),
      password: password.clone(),
    };

    if let Ok(user) =
      circus_common::repo::users::authenticate(&state.pool, &creds).await
    {
      crate::audit::record_with_actor(
        &state.pool,
        &circus_common::audit::Actor::user(user.id, user.username.clone()),
        None,
        "LOGIN_SUCCESS",
        Some("dashboard"),
        Some(&user.id.to_string()),
        serde_json::json!({ "method": "password" }),
      )
      .await;

      let session_id = Uuid::new_v4().to_string();
      state
        .sessions
        .insert(session_id.clone(), crate::state::SessionData {
          api_key:    None,
          user:       Some(user),
          created_at: std::time::Instant::now(),
        });

      let security_flags =
        crate::routes::cookie_security_flags(&state.config.server);
      let cookie = format!(
        "circus_user_session={session_id}; {security_flags}; Path=/; \
         Max-Age=86400"
      );
      return (
        [(axum::http::header::SET_COOKIE, cookie)],
        Redirect::to("/"),
      )
        .into_response();
    }
    crate::audit::record_with_actor(
      &state.pool,
      &circus_common::audit::Actor::anonymous(),
      None,
      "LOGIN_FAILURE",
      Some("dashboard"),
      Some(username),
      serde_json::json!({ "method": "password" }),
    )
    .await;

    let tmpl = LoginTemplate {
      error: Some("Invalid username or password".to_string()),
    };
    return (
      StatusCode::UNAUTHORIZED,
      Html(
        tmpl
          .render()
          .unwrap_or_else(|e| format!("Template error: {e}")),
      ),
    )
      .into_response();
  }

  // Fall back to API key authentication
  if let Some(token) = form.api_key.as_ref() {
    let token = token.trim();
    if token.is_empty() {
      let tmpl = LoginTemplate {
        error: Some("API key is required".to_string()),
      };
      return Html(
        tmpl
          .render()
          .unwrap_or_else(|e| format!("Template error: {e}")),
      )
      .into_response();
    }

    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let key_hash = hex::encode(hasher.finalize());

    if let Ok(Some(api_key)) =
      circus_common::repo::api_keys::get_by_hash(&state.pool, &key_hash).await
    {
      crate::audit::record_with_actor(
        &state.pool,
        &circus_common::audit::Actor::api_key(api_key.id, api_key.name.clone()),
        None,
        "LOGIN_SUCCESS",
        Some("dashboard"),
        Some(&api_key.id.to_string()),
        serde_json::json!({ "method": "api_key" }),
      )
      .await;

      let session_id = Uuid::new_v4().to_string();
      state
        .sessions
        .insert(session_id.clone(), crate::state::SessionData {
          api_key:    Some(api_key),
          user:       None,
          created_at: std::time::Instant::now(),
        });

      let security_flags =
        crate::routes::cookie_security_flags(&state.config.server);
      let cookie = format!(
        "circus_session={session_id}; {security_flags}; Path=/; Max-Age=86400"
      );
      (
        [(axum::http::header::SET_COOKIE, cookie)],
        Redirect::to("/"),
      )
        .into_response()
    } else {
      crate::audit::record_with_actor(
        &state.pool,
        &circus_common::audit::Actor::anonymous(),
        None,
        "LOGIN_FAILURE",
        Some("dashboard"),
        None,
        serde_json::json!({ "method": "api_key" }),
      )
      .await;

      let tmpl = LoginTemplate {
        error: Some("Invalid API key".to_string()),
      };
      Html(
        tmpl
          .render()
          .unwrap_or_else(|e| format!("Template error: {e}")),
      )
      .into_response()
    }
  } else {
    let tmpl = LoginTemplate {
      error: Some(
        "Please provide either username/password or API key".to_string(),
      ),
    };
    Html(
      tmpl
        .render()
        .unwrap_or_else(|e| format!("Template error: {e}")),
    )
    .into_response()
  }
}

pub(super) async fn logout_action(
  State(state): State<AppState>,
  request: axum::extract::Request,
) -> Response {
  // Remove server-side session for both cookie types
  if let Some(cookie_header) = request
    .headers()
    .get("cookie")
    .and_then(|v| v.to_str().ok())
  {
    // Check for user session
    if let Some(session_id) = cookie_header.split(';').find_map(|pair| {
      let pair = pair.trim();
      let (k, v) = pair.split_once('=')?;
      if k.trim() == "circus_user_session" {
        Some(v.trim().to_string())
      } else {
        None
      }
    }) {
      state.sessions.remove(&session_id);
    }

    // Check for legacy API key session
    if let Some(session_id) = cookie_header.split(';').find_map(|pair| {
      let pair = pair.trim();
      let (k, v) = pair.split_once('=')?;
      if k.trim() == "circus_session" {
        Some(v.trim().to_string())
      } else {
        None
      }
    }) {
      state.sessions.remove(&session_id);
    }
  }

  // Clear both cookies
  let cookies = [
    "circus_user_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
    "circus_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
  ];
  (
    [
      (axum::http::header::SET_COOKIE, cookies[0].to_string()),
      (axum::http::header::SET_COOKIE, cookies[1].to_string()),
    ],
    Redirect::to("/"),
  )
    .into_response()
}
