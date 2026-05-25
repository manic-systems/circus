use axum::{
  extract::{FromRequestParts, Request, State},
  http::{StatusCode, request::Parts},
  middleware::Next,
  response::Response,
};
use circus_common::models::{ApiKey, User};
use sha2::{Digest, Sha256};

use crate::state::AppState;

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

  // Try Bearer token first (API key auth)
  let auth_header = request
    .headers()
    .get("authorization")
    .and_then(|v| v.to_str().ok())
    .map(String::from);

  let token = auth_header
    .as_deref()
    .and_then(|h| h.strip_prefix("Bearer "));

  if let Some(token) = token {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let key_hash = hex::encode(hasher.finalize());

    if let Ok(Some(api_key)) =
      circus_common::repo::api_keys::get_by_hash(&state.pool, &key_hash).await
    {
      // Update last used timestamp asynchronously
      let pool = state.pool.clone();
      let key_id = api_key.id;
      tokio::spawn(async move {
        if let Err(e) =
          circus_common::repo::api_keys::touch_last_used(&pool, key_id).await
        {
          tracing::warn!(error = %e, "Failed to update API key last_used timestamp");
        }
      });

      request.extensions_mut().insert(api_key.clone());
      request.extensions_mut().insert(crate::state::SessionData {
        api_key:    Some(api_key),
        user:       None,
        created_at: std::time::Instant::now(),
      });
      return Ok(next.run(request).await);
    }
  }

  // Fall back to session cookie
  if let Some(cookie_header) = request
    .headers()
    .get("cookie")
    .and_then(|v| v.to_str().ok())
  {
    // Try user session first (new circus_user_session cookie)
    if let Some(session_id) = parse_cookie(cookie_header, "circus_user_session")
      && let Some(session) = state.sessions.get(&session_id)
    {
      // Check session expiry (24 hours)
      if session.created_at.elapsed() < std::time::Duration::from_hours(24) {
        // Insert both user and session data
        if let Some(ref user) = session.user {
          request.extensions_mut().insert(user.clone());
        }
        if let Some(ref api_key) = session.api_key {
          request.extensions_mut().insert(api_key.clone());
        }
        return Ok(next.run(request).await);
      }
      // Expired, remove it
      drop(session);
      state.sessions.remove(&session_id);
    }

    // Try legacy API key session (circus_session cookie)
    if let Some(session_id) = parse_cookie(cookie_header, "circus_session")
      && let Some(session) = state.sessions.get(&session_id)
    {
      // Check session expiry (24 hours)
      if session.created_at.elapsed() < std::time::Duration::from_hours(24) {
        if let Some(ref api_key) = session.api_key {
          request.extensions_mut().insert(api_key.clone());
        }
        return Ok(next.run(request).await);
      }
      // Expired, remove it
      drop(session);
      state.sessions.remove(&session_id);
    }
  }

  // No valid auth found
  if is_read {
    Ok(next.run(request).await)
  } else {
    Err(StatusCode::UNAUTHORIZED)
  }
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
      && user.role == "admin"
    {
      // Create a synthetic API key for compatibility
      return Ok(Self(ApiKey {
        id:           user.id,
        name:         user.username.clone(),
        key_hash:     String::new(),
        role:         user.role.clone(),
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

    if key.role == "admin" {
      Ok(Self(key))
    } else {
      Err(StatusCode::FORBIDDEN)
    }
  }
}

/// Extractor that requires one of the specified roles (admin always passes).
/// Use as: `RequireRoles::check(&extensions, &["cancel-build",
/// "restart-jobs"])`
pub struct RequireRoles;

impl RequireRoles {
  /// Check if the session has one of the allowed roles. Admin always passes.
  ///
  /// # Errors
  ///
  /// Returns unauthorized or forbidden status if authentication fails or role
  /// is insufficient.
  pub fn check(
    extensions: &axum::http::Extensions,
    allowed: &[&str],
  ) -> Result<ApiKey, StatusCode> {
    // Check for user first
    if let Some(user) = extensions.get::<User>()
      && (user.role == "admin" || allowed.contains(&user.role.as_str()))
    {
      return Ok(ApiKey {
        id:           user.id,
        name:         user.username.clone(),
        key_hash:     String::new(),
        role:         user.role.clone(),
        created_at:   user.created_at,
        last_used_at: user.last_login_at,
        user_id:      Some(user.id),
      });
    }

    // Fall back to API key
    let key = extensions
      .get::<ApiKey>()
      .cloned()
      .ok_or(StatusCode::UNAUTHORIZED)?;

    if key.role == "admin" || allowed.contains(&key.role.as_str()) {
      Ok(key)
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
  // Try Bearer token first (API key auth)
  let auth_header = request
    .headers()
    .get("authorization")
    .and_then(|v| v.to_str().ok())
    .map(String::from);

  if let Some(ref auth_header) = auth_header
    && let Some(token) = auth_header.strip_prefix("Bearer ")
  {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let key_hash = hex::encode(hasher.finalize());

    if let Ok(Some(api_key)) =
      circus_common::repo::api_keys::get_by_hash(&state.pool, &key_hash).await
    {
      // Update last used timestamp asynchronously
      let pool = state.pool.clone();
      let key_id = api_key.id;
      tokio::spawn(async move {
        if let Err(e) =
          circus_common::repo::api_keys::touch_last_used(&pool, key_id).await
        {
          tracing::warn!(error = %e, "Failed to update API key last_used timestamp");
        }
      });

      request.extensions_mut().insert(api_key);
    }
  }

  // Extract cookie header next
  let cookie_header = request
    .headers()
    .get("cookie")
    .and_then(|v| v.to_str().ok())
    .map(std::string::ToString::to_string);

  if let Some(cookie_header) = cookie_header {
    // Try user session first
    if let Some(session_id) =
      parse_cookie(&cookie_header, "circus_user_session")
      && let Some(session) = state.sessions.get(&session_id)
    {
      // Check session expiry
      if session.created_at.elapsed() < std::time::Duration::from_hours(24) {
        if let Some(ref user) = session.user {
          request.extensions_mut().insert(user.clone());
        }
        if let Some(ref api_key) = session.api_key {
          request.extensions_mut().insert(api_key.clone());
        }
        let token = state.csrf_token_for(&session_id);
        request
          .extensions_mut()
          .insert(crate::state::CsrfToken(token));
      } else {
        drop(session);
        state.sessions.remove(&session_id);
      }
    }

    // Try legacy API key session
    if let Some(session_id) = parse_cookie(&cookie_header, "circus_session")
      && let Some(session) = state.sessions.get(&session_id)
    {
      // Check session expiry
      if session.created_at.elapsed() < std::time::Duration::from_hours(24) {
        if let Some(ref api_key) = session.api_key {
          request.extensions_mut().insert(api_key.clone());
        }
        let token = state.csrf_token_for(&session_id);
        request
          .extensions_mut()
          .insert(crate::state::CsrfToken(token));
      } else {
        drop(session);
        state.sessions.remove(&session_id);
      }
    }
  }

  next.run(request).await
}

fn parse_cookie(header: &str, name: &str) -> Option<String> {
  header.split(';').find_map(|pair| {
    let pair = pair.trim();
    let (k, v) = pair.split_once('=')?;
    if k.trim() == name {
      Some(v.trim().to_string())
    } else {
      None
    }
  })
}
