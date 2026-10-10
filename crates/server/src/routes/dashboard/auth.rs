//! Dashboard login/logout handlers.
//!
//! Login accepts either user credentials (username + password) or a raw
//! API key. Successful logins set a session cookie with the configured
//! `Secure` flag policy. Logout drops both legacy cookie names so users
//! coming off an older session are fully cleaned up.

use axum::{
  Extension,
  Form,
  extract::{Path, State},
  http::StatusCode,
  response::{IntoResponse, Redirect, Response},
};
use circus_common::models::User;
use data_encoding::HEXLOWER;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
  routes::{
    dashboard::{
      admin::CsrfOnlyForm,
      shared::DashboardContext,
      views::account::SignInError,
    },
    oidc,
  },
  session_cookie::{
    API_KEY_SESSION_COOKIE,
    OIDC_PROVIDER_COOKIE,
    USER_SESSION_COOKIE,
    api_key_session_cookie,
    clear_cookie,
    cookie_value,
    user_session_cookie,
  },
  state::AppState,
};

#[derive(serde::Deserialize)]
pub(super) struct LoginForm {
  username: Option<String>,
  api_key:  Option<String>,
  password: Option<String>,
  next:     Option<String>,
}

pub(super) async fn login_action(
  State(state): State<AppState>,
  Form(form): Form<LoginForm>,
) -> Response {
  let next = crate::routes::return_to(form.next.as_deref())
    .unwrap_or("/")
    .to_owned();
  // Try username/password authentication first
  if let (Some(username), Some(password)) =
    (form.username.as_ref(), form.password.as_ref())
  {
    if !state.config.server.password_login {
      return failed(SignInError::PasswordDisabled, &form);
    }

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

      let session = match circus_common::repo::users::create_session(
        &state.pool,
        user.id,
      )
      .await
      {
        Ok(session) => session,
        Err(e) => {
          tracing::error!(user_id = %user.id, "failed to create user session: {e}");
          return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        },
      };

      let cookie = user_session_cookie(&session.0, &state.config.server);
      return (
        [(axum::http::header::SET_COOKIE, cookie)],
        Redirect::to(&next),
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

    return failed(SignInError::InvalidPassword, &form);
  }

  // Fall back to API key authentication
  if let Some(token) = form.api_key.as_ref() {
    let token = token.trim();
    if token.is_empty() {
      return failed(SignInError::MissingKey, &form);
    }

    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let key_hash = HEXLOWER.encode(&hasher.finalize());

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

      let cookie = api_key_session_cookie(&session_id, &state.config.server);
      (
        [(axum::http::header::SET_COOKIE, cookie)],
        Redirect::to(&next),
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

      failed(SignInError::InvalidKey, &form)
    }
  } else {
    failed(SignInError::MissingCredentials, &form)
  }
}

fn failed(error: SignInError, form: &LoginForm) -> Response {
  Redirect::to(&error.login_href(form.next.as_deref())).into_response()
}

pub(super) async fn logout_action(
  State(state): State<AppState>,
  request: axum::extract::Request,
) -> Response {
  // Remove server-side session for both cookie types
  {
    // Check for user session
    if let Some(session_id) =
      cookie_value(request.headers(), USER_SESSION_COOKIE)
      && let Err(e) =
        circus_common::repo::users::delete_session(&state.pool, &session_id)
          .await
    {
      tracing::warn!("failed to delete user session during logout: {e}");
    }

    // Check for legacy API key session
    if let Some(session_id) =
      cookie_value(request.headers(), API_KEY_SESSION_COOKIE)
    {
      state.sessions.remove(&session_id);
    }
  }

  let provider_logout =
    match cookie_value(request.headers(), OIDC_PROVIDER_COOKIE) {
      Some(provider) => oidc::end_session_url(&state, &provider).await,
      None => None,
    };

  let cookies = [
    clear_cookie(USER_SESSION_COOKIE, &state.config.server),
    clear_cookie(API_KEY_SESSION_COOKIE, &state.config.server),
    clear_cookie(OIDC_PROVIDER_COOKIE, &state.config.server),
  ];
  // Header arrays insert, so only AppendHeaders keeps every cookie.
  (
    axum::response::AppendHeaders(
      cookies.map(|cookie| (axum::http::header::SET_COOKIE, cookie)),
    ),
    Redirect::to(provider_logout.as_deref().unwrap_or("/")),
  )
    .into_response()
}

#[derive(serde::Deserialize)]
pub(super) struct LinkForm {
  csrf_token: String,
  password:   Option<String>,
}

/// Re-checks the password first, so a stolen session alone cannot attach a
/// lasting way in.
pub(super) async fn account_link(
  State(state): State<AppState>,
  Path(provider): Path<String>,
  ctx: DashboardContext,
  user: Option<Extension<User>>,
  Form(form): Form<LinkForm>,
) -> Response {
  let Some(Extension(user)) = user else {
    return Redirect::to("/login").into_response();
  };

  if let Err(e) = ctx.check_csrf(&form.csrf_token) {
    return e.into_response();
  }

  if !state.config.oauth.oidc.contains_key(&provider) {
    return StatusCode::NOT_FOUND.into_response();
  }

  if let Some(hash) = &user.password_hash {
    let password = form.password.as_deref().unwrap_or_default();

    match circus_common::repo::users::verify_password(password, hash) {
      Ok(true) => {},
      Ok(false) => {
        crate::audit::record_with_actor(
          &state.pool,
          &circus_common::audit::Actor::user(user.id, user.username.clone()),
          None,
          "OIDC_LINK_FAILURE",
          Some("oidc"),
          Some(&provider),
          serde_json::json!({ "reason": "incorrect password" }),
        )
        .await;

        return Redirect::to(&format!(
          "/account/link/{provider}?error=incorrect-password"
        ))
        .into_response();
      },
      Err(e) => {
        tracing::error!(user_id = %user.id, "failed to verify password: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
      },
    }
  }

  oidc::start_link(&state, &provider, user.id).await
}

pub(super) async fn account_unlink(
  State(state): State<AppState>,
  Path(provider): Path<String>,
  ctx: DashboardContext,
  user: Option<Extension<User>>,
  Form(form): Form<CsrfOnlyForm>,
) -> Response {
  let Some(Extension(user)) = user else {
    return Redirect::to("/login").into_response();
  };

  if let Err(e) = ctx.check_csrf(&form.csrf_token) {
    return e.into_response();
  }

  if !state.config.oauth.oidc.contains_key(&provider) {
    return StatusCode::NOT_FOUND.into_response();
  }

  match circus_common::repo::users::unlink_identity(
    &state.pool,
    user.id,
    &provider,
  )
  .await
  {
    Ok(true) => {},
    Ok(false) => return Redirect::to("/account").into_response(),
    Err(e) => {
      tracing::error!(user_id = %user.id, %provider, "failed to unlink identity: {e}");
      return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    },
  }

  crate::audit::record_with_actor(
    &state.pool,
    &circus_common::audit::Actor::user(user.id, user.username),
    None,
    "OIDC_UNLINK",
    Some("oidc"),
    Some(&provider),
    serde_json::json!({ "provider": provider }),
  )
  .await;

  Redirect::to("/account").into_response()
}
