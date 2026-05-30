use std::{sync::Arc, time::Instant};

use circus_common::{
  config::Config,
  models::{ApiKey, User},
};
use dashmap::DashMap;
use hmac::KeyInit;
use regex::Regex;
use sqlx::PgPool;

/// Maximum session lifetime before automatic eviction (24 hours).
const SESSION_MAX_AGE: std::time::Duration =
  std::time::Duration::from_hours(24);

/// How often the background cleanup task runs (every 5 minutes).
const SESSION_CLEANUP_INTERVAL: std::time::Duration =
  std::time::Duration::from_mins(5);

/// How long a cached narinfo stays in memory before eviction.
const NARINFO_CACHE_TTL: std::time::Duration =
  std::time::Duration::from_hours(1);

/// Hard cap on the number of cached narinfos. Excess entries are evicted
/// on the next sweep regardless of TTL.
const NARINFO_CACHE_MAX_ENTRIES: usize = 50_000;

/// Session data supporting both API key and user authentication
#[derive(Clone)]
pub struct SessionData {
  pub api_key:    Option<ApiKey>,
  pub user:       Option<User>,
  pub created_at: Instant,
}

impl SessionData {
  /// Check if the session has admin role
  #[must_use]
  pub fn is_admin(&self) -> bool {
    self.user.as_ref().map_or_else(
      || self.api_key.as_ref().is_some_and(|key| key.role == "admin"),
      |user| user.role == "admin",
    )
  }

  /// Check if the session has a specific role
  #[must_use]
  pub fn has_role(&self, role: &str) -> bool {
    if self.is_admin() {
      return true;
    }
    self.user.as_ref().map_or_else(
      || self.api_key.as_ref().is_some_and(|key| key.role == role),
      |user| user.role == role,
    )
  }

  /// Get the display name for the session (username or api key name)
  #[must_use]
  pub fn display_name(&self) -> String {
    self.user.as_ref().map_or_else(
      || {
        self
          .api_key
          .as_ref()
          .map_or_else(|| "Anonymous".to_string(), |key| key.name.clone())
      },
      |user| user.username.clone(),
    )
  }

  /// Check if this is a user session (not just API key)
  #[must_use]
  pub const fn is_user_session(&self) -> bool {
    self.user.is_some()
  }
}

/// Cached narinfo body together with the instant it was inserted.
/// Used by the background eviction task to drop entries past
/// `NARINFO_CACHE_TTL`.
#[derive(Clone)]
pub struct CachedNarinfo {
  pub body:       String,
  pub created_at: Instant,
}

#[derive(Clone)]
pub struct AppState {
  pub pool:          PgPool,
  pub config:        Config,
  pub sessions:      Arc<DashMap<String, SessionData>>,
  pub narinfo_cache: Arc<DashMap<String, CachedNarinfo>>,
  pub http_client:   reqwest::Client,
  /// Per-process key used to derive CSRF tokens from session IDs via HMAC.
  /// Regenerated on every restart, which invalidates outstanding tokens; the
  /// dashboard re-issues them on the next page render so this is benign.
  pub csrf_secret:   Arc<[u8; 32]>,
  /// Compiled email validation regex from `server.email_validation_regex`.
  /// `None` means only structural checks (non-empty, contains `@`).
  pub email_regex:   Option<Arc<Regex>>,
}

impl AppState {
  /// Compute the CSRF token bound to a given session ID. Same input always
  /// produces the same output for the lifetime of the process; comparing
  /// with [`subtle::ConstantTimeEq`] avoids timing leaks.
  ///
  /// # Panics
  ///
  /// Panics if the HMAC key length is rejected by `Hmac::<Sha256>`.
  /// The key is always 32 bytes, which SHA-256 accepts.
  #[must_use]
  pub fn csrf_token_for(&self, session_id: &str) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    #[expect(
      clippy::expect_used,
      reason = "32-byte key is always valid for HMAC-SHA256"
    )]
    let mut mac = Hmac::<Sha256>::new_from_slice(self.csrf_secret.as_ref())
      .expect("HMAC-SHA256 accepts any key length");
    mac.update(session_id.as_bytes());
    hex::encode(mac.finalize().into_bytes())
  }
}

/// Marker placed in request extensions so dashboard handlers can render
/// the CSRF token in templates and validate it on POSTs without re-deriving
/// it from the session cookie themselves.
#[derive(Clone, Debug)]
pub struct CsrfToken(pub String);

impl AppState {
  /// Spawn a background task that periodically evicts expired sessions.
  /// This prevents unbounded memory growth from the in-memory session store.
  pub fn spawn_session_cleanup(&self) {
    let sessions = Arc::clone(&self.sessions);
    tokio::spawn(async move {
      loop {
        tokio::time::sleep(SESSION_CLEANUP_INTERVAL).await;
        let before = sessions.len();
        sessions
          .retain(|_, session| session.created_at.elapsed() < SESSION_MAX_AGE);
        let evicted = before.saturating_sub(sessions.len());
        if evicted > 0 {
          tracing::debug!(
            evicted = evicted,
            remaining = sessions.len(),
            "Evicted expired sessions"
          );
        }
      }
    });
  }

  /// Spawn a background task that evicts narinfo cache entries past the TTL
  /// and trims the map back to the size cap. Without this the cache grows
  /// without bound on a busy mirror.
  pub fn spawn_narinfo_cleanup(&self) {
    let cache = Arc::clone(&self.narinfo_cache);
    tokio::spawn(async move {
      loop {
        tokio::time::sleep(SESSION_CLEANUP_INTERVAL).await;
        cache.retain(|_, v| v.created_at.elapsed() < NARINFO_CACHE_TTL);
        if cache.len() > NARINFO_CACHE_MAX_ENTRIES {
          // Over the hard cap: drop the oldest entries until under the limit.
          let mut entries: Vec<(String, Instant)> = cache
            .iter()
            .map(|e| (e.key().clone(), e.value().created_at))
            .collect();
          entries.sort_by_key(|(_, t)| *t);
          let to_drop = cache.len().saturating_sub(NARINFO_CACHE_MAX_ENTRIES);
          for (k, _) in entries.into_iter().take(to_drop) {
            cache.remove(&k);
          }
        }
      }
    });
  }
}
