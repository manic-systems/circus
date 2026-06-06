use std::{path::PathBuf, sync::Arc, time::Instant};

use circus_common::{
  config::Config,
  models::{ApiKey, User},
};
use dashmap::DashMap;
use harmonia_store_path::StoreDir;
use hmac::KeyInit;
use moka::sync::Cache;
use regex::Regex;
use sqlx::{ConnectOptions as _, PgPool, SqlitePool};

/// Maximum lifetime for legacy in-memory API-key dashboard sessions.
const SESSION_MAX_AGE: std::time::Duration =
  std::time::Duration::from_hours(24);

/// How often the background cleanup task runs (every 5 minutes).
const SESSION_CLEANUP_INTERVAL: std::time::Duration =
  std::time::Duration::from_mins(5);

/// How long a cached narinfo stays in memory before eviction.
const NARINFO_CACHE_TTL: std::time::Duration =
  std::time::Duration::from_hours(1);

/// Hard cap on the number of cached narinfos.
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

pub type NarinfoCache = Cache<String, String>;

#[derive(Clone)]
pub struct NixStore {
  store_dir: PathBuf,
}

impl NixStore {
  #[must_use]
  pub fn new(store_dir: PathBuf) -> Self {
    Self { store_dir }
  }

  pub fn store_dir(&self) -> Result<StoreDir, String> {
    StoreDir::new(self.store_dir.clone()).map_err(|e| e.to_string())
  }

  fn db_path(&self) -> Option<PathBuf> {
    self
      .store_dir
      .parent()
      .map(|root| root.join("var/nix/db/db.sqlite"))
  }

  /// Open the local Nix store database once for binary-cache serving.
  ///
  /// Missing databases are treated as cache misses, but an existing database
  /// that cannot be opened is returned as an error so the failure is explicit
  /// at startup instead of surfacing as per-request 500s.
  pub async fn open_db(&self) -> Result<Option<SqlitePool>, sqlx::Error> {
    let Some(db_path) = self.db_path() else {
      return Ok(None);
    };
    if !db_path.exists() {
      return Ok(None);
    }

    let options = sqlx::sqlite::SqliteConnectOptions::new()
      .filename(&db_path)
      .read_only(true)
      .create_if_missing(false)
      .disable_statement_logging();

    SqlitePool::connect_with(options).await.map(Some)
  }
}

#[derive(Clone)]
pub struct AppState {
  pub pool:          PgPool,
  pub nix_store:     NixStore,
  pub nix_store_db:  Option<SqlitePool>,
  pub config:        Config,
  pub sessions:      Arc<DashMap<String, SessionData>>,
  pub narinfo_cache: NarinfoCache,
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
  #[must_use]
  pub fn new_narinfo_cache() -> NarinfoCache {
    Cache::builder()
      .max_capacity(NARINFO_CACHE_MAX_ENTRIES as u64)
      .time_to_live(NARINFO_CACHE_TTL)
      .build()
  }

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
  /// Spawn a background task that periodically evicts expired legacy API-key
  /// dashboard sessions. User sessions are validated against `PostgreSQL`.
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
}
