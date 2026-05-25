// Wraps `sqlx::postgres::PgListener` to subscribe to notification channels
// and signal an `Arc<tokio::sync::Notify>` when events arrive. Daemons use
// this to wake immediately instead of waiting for the next poll interval.
use std::sync::Arc;

use sqlx::PgPool;
use tokio::{sync::Notify, task::JoinHandle};

/// Channel emitted on `builds` INSERT or status UPDATE.
pub const CHANNEL_BUILDS_CHANGED: &str = "circus_builds_changed";

/// Channel emitted on `jobsets` INSERT, UPDATE (relevant fields), or DELETE.
pub const CHANNEL_JOBSETS_CHANGED: &str = "circus_jobsets_changed";

/// Spawns a background task that listens on the given PG channels and calls
/// `wakeup.notify_waiters()` on each notification. Reconnects with 5s backoff
/// on connection loss.
pub fn spawn_listener(
  pool: &PgPool,
  channels: &[&str],
  wakeup: Arc<Notify>,
) -> JoinHandle<()> {
  let pool = pool.clone();
  let channels: Vec<String> =
    channels.iter().map(|s| (*s).to_owned()).collect();

  tokio::spawn(async move {
    loop {
      if let Err(e) = listen_loop(&pool, &channels, &wakeup).await {
        tracing::warn!("PG LISTEN connection lost: {e}, reconnecting in 5s");
      }
      tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
  })
}

/// Core listen loop: connects, subscribes, and dispatches notifications.
async fn listen_loop(
  pool: &PgPool,
  channels: &[String],
  wakeup: &Notify,
) -> Result<(), sqlx::Error> {
  let mut listener = sqlx::postgres::PgListener::connect_with(pool).await?;

  let channel_refs: Vec<&str> = channels.iter().map(String::as_str).collect();
  listener.listen_all(channel_refs).await?;

  tracing::info!(channels = ?channels, "PG LISTEN subscribed");

  loop {
    listener.recv().await?;
    wakeup.notify_waiters();
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn channel_names_are_valid_pg_identifiers() {
    for name in [CHANNEL_BUILDS_CHANGED, CHANNEL_JOBSETS_CHANGED] {
      assert!(name.len() < 64, "channel name too long: {name}");
      assert!(!name.contains(' '), "channel name has spaces: {name}");
      assert!(
        name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
        "channel name has invalid chars: {name}"
      );
    }
  }

  #[test]
  fn channel_names_match_migration_triggers() {
    // These must match the pg_notify() calls in migration 015
    assert_eq!(CHANNEL_BUILDS_CHANGED, "circus_builds_changed");
    assert_eq!(CHANNEL_JOBSETS_CHANGED, "circus_jobsets_changed");
  }
}
