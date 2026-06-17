use axum::{
  extract::{FromRequestParts, Request, State},
  http::{StatusCode, request::Parts},
  middleware::Next,
  response::Response,
};
use circus_common::{
  models::{ApiKey, User},
  repo,
  roles::GlobalRole,
};
use sha2::{Digest, Sha256};

use crate::{
  session_cookie::{
    API_KEY_SESSION_COOKIE,
    API_KEY_SESSION_MAX_AGE,
    USER_SESSION_COOKIE,
    cookie_value,
  },
  state::{AppState, CsrfToken, SessionData},
};

struct UserSession {
  session_id: String,
  user:       User,
}

struct LegacyApiKeySession {
  session_id: String,
  api_key:    Option<ApiKey>,
}

#[derive(Default)]
struct RequestAuth {
  bearer_api_key: Option<ApiKey>,
  user_session:   Option<UserSession>,
  legacy_session: Option<LegacyApiKeySession>,
}

#[derive(Default)]
struct RequestCredentials {
  bearer_token:          Option<String>,
  user_session_id:       Option<String>,
  legacy_api_session_id: Option<String>,
}

impl RequestCredentials {
  fn from_request(request: &Request) -> Self {
    Self {
      bearer_token:          request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .map(str::to_owned),
      user_session_id:       cookie_value(
        request.headers(),
        USER_SESSION_COOKIE,
      ),
      legacy_api_session_id: cookie_value(
        request.headers(),
        API_KEY_SESSION_COOKIE,
      ),
    }
  }
}

impl RequestAuth {
  async fn resolve(state: &AppState, credentials: RequestCredentials) -> Self {
    Self {
      bearer_api_key: resolve_bearer_api_key(state, credentials.bearer_token)
        .await,
      user_session:   resolve_user_session(state, credentials.user_session_id)
        .await,
      legacy_session: resolve_legacy_api_key_session(
        state,
        credentials.legacy_api_session_id,
      ),
    }
  }
}

/// Extract and validate an API key from the Authorization header or session
/// cookie. Keys use the format: `Bearer circus_xxxx`. Session cookies use
/// `circus_session=<id>` for API keys or `circus_user_session=<id>` for users.
/// Write endpoints (POST/PUT/DELETE/PATCH) require a valid key.
/// Read endpoints (GET/HEAD/OPTIONS) try to extract optionally (for
/// dashboard admin UI).
///
/// # Errors
///
/// Returns unauthorized status if no valid authentication is found for write
/// operations.
pub async fn require_api_key(
  State(state): State<AppState>,
  mut request: Request,
  next: Next,
) -> Result<Response, StatusCode> {
  let method = request.method().clone();
  let is_read = method == axum::http::Method::GET
    || method == axum::http::Method::HEAD
    || method == axum::http::Method::OPTIONS;

  let credentials = RequestCredentials::from_request(&request);
  let auth = RequestAuth::resolve(&state, credentials).await;

  if let Some(api_key) = auth.bearer_api_key {
    request.extensions_mut().insert(api_key.clone());
    request.extensions_mut().insert(SessionData {
      api_key:    Some(api_key),
      user:       None,
      created_at: std::time::Instant::now(),
    });
    return Ok(next.run(request).await);
  }

  // Fall back to session cookie. Mutating API requests authenticated by a
  // browser session must carry the dashboard CSRF token. Bearer-token API calls
  // are unaffected because they are not sent automatically by browsers.
  if let Some(session) = auth.user_session {
    if !is_read && !valid_csrf_header(&state, &request, &session.session_id) {
      return Err(StatusCode::FORBIDDEN);
    }
    request.extensions_mut().insert(session.user.clone());
    request.extensions_mut().insert(SessionData {
      api_key:    None,
      user:       Some(session.user),
      created_at: std::time::Instant::now(),
    });
    return Ok(next.run(request).await);
  }

  if let Some(session) = auth.legacy_session {
    if !is_read && !valid_csrf_header(&state, &request, &session.session_id) {
      return Err(StatusCode::FORBIDDEN);
    }
    if let Some(api_key) = session.api_key {
      request.extensions_mut().insert(api_key);
    }
    return Ok(next.run(request).await);
  }

  // No valid auth found
  if is_read && !state.config.server.require_api_key_for_reads {
    Ok(next.run(request).await)
  } else {
    Err(StatusCode::UNAUTHORIZED)
  }
}

fn valid_csrf_header(
  state: &AppState,
  request: &Request,
  session_id: &str,
) -> bool {
  use subtle::ConstantTimeEq;
  let expected = state.csrf_token_for(session_id);
  request
    .headers()
    .get("x-csrf-token")
    .and_then(|v| v.to_str().ok())
    .is_some_and(|submitted| {
      expected.as_bytes().ct_eq(submitted.as_bytes()).unwrap_u8() == 1
    })
}

/// Extractor that requires an authenticated admin user.
/// Use as a handler parameter: `_auth: RequireAdmin`
pub struct RequireAdmin(pub ApiKey);

impl FromRequestParts<AppState> for RequireAdmin {
  type Rejection = StatusCode;

  async fn from_request_parts(
    parts: &mut Parts,
    _state: &AppState,
  ) -> Result<Self, Self::Rejection> {
    // Check for user first (new auth)
    if let Some(user) = parts.extensions.get::<User>()
      && user.role == GlobalRole::Admin
    {
      // Create a synthetic API key for compatibility
      return Ok(Self(ApiKey {
        id:           user.id,
        name:         user.username.clone(),
        key_hash:     String::new(),
        role:         user.role,
        created_at:   user.created_at,
        last_used_at: user.last_login_at,
        user_id:      Some(user.id),
      }));
    }

    // Fall back to API key
    let key = parts
      .extensions
      .get::<ApiKey>()
      .cloned()
      .ok_or(StatusCode::UNAUTHORIZED)?;

    if key.role == GlobalRole::Admin {
      Ok(Self(key))
    } else {
      Err(StatusCode::FORBIDDEN)
    }
  }
}

/// Session extraction middleware for dashboard routes.
/// Reads `circus_user_session` or `circus_session` cookie, or Bearer token (API
/// key), and inserts User/ApiKey into extensions if valid.
pub async fn extract_session(
  State(state): State<AppState>,
  mut request: Request,
  next: Next,
) -> Response {
  let credentials = RequestCredentials::from_request(&request);
  let auth = RequestAuth::resolve(&state, credentials).await;

  if let Some(api_key) = auth.bearer_api_key {
    request.extensions_mut().insert(api_key);
  }

  if let Some(session) = auth.user_session {
    request.extensions_mut().insert(session.user);
    request
      .extensions_mut()
      .insert(CsrfToken(state.csrf_token_for(&session.session_id)));
  }

  if let Some(session) = auth.legacy_session {
    if let Some(api_key) = session.api_key {
      request.extensions_mut().insert(api_key);
    }
    request
      .extensions_mut()
      .insert(CsrfToken(state.csrf_token_for(&session.session_id)));
  }

  next.run(request).await
}

async fn resolve_bearer_api_key(
  state: &AppState,
  token: Option<String>,
) -> Option<ApiKey> {
  let token = token?;
  let mut hasher = Sha256::new();
  hasher.update(token.as_bytes());
  let key_hash = hex::encode(hasher.finalize());

  match circus_common::repo::api_keys::get_by_hash(&state.pool, &key_hash).await
  {
    Ok(Some(api_key)) => {
      touch_api_key_last_used(state, &api_key);
      Some(api_key)
    },
    Ok(None) => None,
    Err(e) => {
      tracing::warn!("failed to validate API key: {e}");
      None
    },
  }
}

async fn resolve_user_session(
  state: &AppState,
  session_id: Option<String>,
) -> Option<UserSession> {
  let session_id = session_id?;
  match repo::users::validate_session(&state.pool, &session_id).await {
    Ok(Some(user)) => Some(UserSession { session_id, user }),
    Ok(None) => None,
    Err(e) => {
      tracing::warn!("failed to validate user session: {e}");
      None
    },
  }
}

fn resolve_legacy_api_key_session(
  state: &AppState,
  session_id: Option<String>,
) -> Option<LegacyApiKeySession> {
  let session_id = session_id?;
  let session = state.sessions.get(&session_id)?;
  if session.created_at.elapsed() < API_KEY_SESSION_MAX_AGE {
    return Some(LegacyApiKeySession {
      session_id,
      api_key: session.api_key.clone(),
    });
  }

  drop(session);
  state.sessions.remove(&session_id);
  None
}

fn touch_api_key_last_used(state: &AppState, api_key: &ApiKey) {
  let pool = state.pool.clone();
  let key_id = api_key.id;
  tokio::spawn(async move {
    if let Err(e) =
      circus_common::repo::api_keys::touch_last_used(&pool, key_id).await
    {
      tracing::warn!(error = %e, "Failed to update API key last_used timestamp");
    }
  });
}
