//! Dashboard login/logout handlers.
//!
//! Login accepts either user credentials (username + password) or a raw
//! API key. Successful logins set a session cookie with the configured
//! `Secure` flag policy. Logout drops both legacy cookie names so users
//! coming off an older session are fully cleaned up.

use askama::Template;
use axum::{
  Extension,
  Form,
  extract::{Path, Query, State},
  http::StatusCode,
  response::{Html, IntoResponse, Redirect, Response},
};
use circus_common::models::User;
use data_encoding::HEXLOWER;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
  routes::{
    dashboard::{
      admin::CsrfOnlyForm,
      shared::{DashboardContext, RenderExt, UiTemplateConfig},
      templates::{
        AccountLinkTemplate,
        AccountProvider,
        AccountTemplate,
        LinkStatus,
        LoginTemplate,
      },
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
pub(super) struct LoginQuery {
  next:  Option<String>,
  /// Show the page even when it would hand straight off to a provider, so an
  /// API key can still be entered.
  local: Option<String>,
}

pub(super) async fn login_page(
  State(state): State<AppState>,
  Query(query): Query<LoginQuery>,
) -> Response {
  let tmpl =
    LoginTemplate::new(&state.config, None).with_next(query.next.as_deref());

  if let [provider] = tmpl.providers.as_slice()
    && !state.config.server.password_login
    && query.local.is_none()
  {
    return Redirect::to(&provider.href).into_response();
  }

  Html(
    tmpl
      .render()
      .unwrap_or_else(|e| format!("Template error: {e}")),
  )
  .into_response()
}

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
      let tmpl = LoginTemplate::new(
        &state.config,
        Some("Password sign-in is disabled".into()),
      )
      .with_next(form.next.as_deref());
      return (
        StatusCode::FORBIDDEN,
        Html(
          tmpl
            .render()
            .unwrap_or_else(|e| format!("Template error: {e}")),
        ),
      )
        .into_response();
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

    let tmpl = LoginTemplate::new(
      &state.config,
      Some("Invalid username or password".into()),
    )
    .with_next(form.next.as_deref());
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
      let tmpl =
        LoginTemplate::new(&state.config, Some("API key is required".into()))
          .with_next(form.next.as_deref());
      return Html(
        tmpl
          .render()
          .unwrap_or_else(|e| format!("Template error: {e}")),
      )
      .into_response();
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

      let tmpl =
        LoginTemplate::new(&state.config, Some("Invalid API key".into()))
          .with_next(form.next.as_deref());
      Html(
        tmpl
          .render()
          .unwrap_or_else(|e| format!("Template error: {e}")),
      )
      .into_response()
    }
  } else {
    let tmpl = LoginTemplate::new(
      &state.config,
      Some("Please provide either username/password or API key".into()),
    )
    .with_next(form.next.as_deref());
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

pub(super) async fn account_page(
  State(state): State<AppState>,
  ctx: DashboardContext,
  user: Option<Extension<User>>,
) -> Response {
  let Some(Extension(user)) = user else {
    return Redirect::to("/login?next=/account").into_response();
  };

  let linked = match circus_common::repo::users::linked_providers(
    &state.pool,
    user.id,
  )
  .await
  {
    Ok(linked) => linked,
    Err(e) => {
      tracing::error!(user_id = %user.id, "failed to list linked identities: {e}");
      return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    },
  };
  let native_issuer = match circus_common::repo::users::native_external_id(
    &state.pool,
    user.id,
  )
  .await
  {
    Ok(external_id) => {
      external_id
        .and_then(|id| id.rsplit_once('#').map(|(issuer, _)| issuer.to_owned()))
    },
    Err(e) => {
      tracing::error!(user_id = %user.id, "failed to read account identity: {e}");
      return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    },
  };
  let providers = state
    .config
    .oauth
    .oidc
    .iter()
    .map(|(name, provider)| {
      let native = native_issuer.as_deref().is_some_and(|issuer| {
        issuer.trim_end_matches('/')
          == provider.issuer_url.trim_end_matches('/')
      });

      AccountProvider {
        name:   name.clone(),
        label:  provider.display_name.clone(),
        status: if native {
          LinkStatus::Native
        } else if linked.contains(name) {
          LinkStatus::Linked
        } else {
          LinkStatus::Unlinked
        },
      }
    })
    .collect();

  AccountTemplate {
    ui: UiTemplateConfig::from_config(&state.config.ui),
    is_admin: ctx.is_admin,
    auth_name: ctx.auth_name.clone(),
    csrf_token: ctx.csrf_token.clone(),
    role: user.role.to_string(),
    has_password: state.config.server.password_login
      && user.password_hash.is_some(),
    username: user.username,
    providers,
  }
  .render_html_or_500()
  .into_response()
}

pub(super) async fn account_link_page(
  State(state): State<AppState>,
  Path(provider): Path<String>,
  ctx: DashboardContext,
  user: Option<Extension<User>>,
) -> Response {
  let Some(Extension(user)) = user else {
    return Redirect::to("/login").into_response();
  };

  render_link_page(&state, &ctx, &user, &provider, None)
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

        return render_link_page(
          &state,
          &ctx,
          &user,
          &provider,
          Some("Incorrect password"),
        );
      },
      Err(e) => {
        tracing::error!(user_id = %user.id, "failed to verify password: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
      },
    }
  }

  oidc::start_link(&state, &provider, user.id).await
}

fn render_link_page(
  state: &AppState,
  ctx: &DashboardContext,
  user: &User,
  provider: &str,
  error: Option<&str>,
) -> Response {
  let Some(config) = state.config.oauth.oidc.get(provider) else {
    return StatusCode::NOT_FOUND.into_response();
  };

  let page = AccountLinkTemplate {
    ui:           UiTemplateConfig::from_config(&state.config.ui),
    is_admin:     ctx.is_admin,
    auth_name:    ctx.auth_name.clone(),
    csrf_token:   ctx.csrf_token.clone(),
    username:     user.username.clone(),
    name:         provider.to_owned(),
    label:        config.display_name.clone(),
    has_password: user.password_hash.is_some(),
    error:        error.map(str::to_owned),
  }
  .render_html_or_500();
  let status = if error.is_some() {
    StatusCode::UNAUTHORIZED
  } else {
    StatusCode::OK
  };

  (status, page).into_response()
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
