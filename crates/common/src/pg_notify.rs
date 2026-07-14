// Deadpool consumes async messages internally, so LISTEN needs its own client.
use std::{sync::Arc, time::Duration};

use futures::{StreamExt as _, stream};
use tokio::{sync::Notify, task::JoinHandle};
use tokio_postgres::{AsyncMessage, NoTls};

use crate::db::{PgPool, TlsMode};

/// Channel emitted on `builds` INSERT or status UPDATE.
pub const CHANNEL_BUILDS_CHANGED: &str = "circus_builds_changed";

/// Channel emitted on `jobsets` INSERT, UPDATE (relevant fields), or DELETE.
pub const CHANNEL_JOBSETS_CHANGED: &str = "circus_jobsets_changed";

/// Send an empty notification on `channel` to wake its listeners.
///
/// # Errors
///
/// Returns the underlying pool or database error.
pub async fn notify(pool: &PgPool, channel: &str) -> crate::error::Result<()> {
  let client = pool.get().await?;
  circus_codegen::queries::database::notify()
    .bind(&client, &channel)
    .one()
    .await?;
  Ok(())
}

/// Spawns a background task that listens on the given PG channels and signals
/// `wakeup` on each notification. Uses `notify_one` so a notification arriving
/// while the daemon is mid-cycle still wakes the next `.notified()` await.
/// Reconnects with 5s backoff on connection loss.
pub fn spawn_listener(
  database_url: &str,
  channels: &[&str],
  wakeup: Arc<Notify>,
) -> JoinHandle<()> {
  let database_url = database_url.to_owned();
  let channels: Vec<String> =
    channels.iter().map(|s| (*s).to_owned()).collect();

  tokio::spawn(async move {
    loop {
      if let Err(e) = listen_loop(&database_url, &channels, &wakeup).await {
        tracing::warn!("PG LISTEN connection lost: {e}, reconnecting in 5s");
      }
      tokio::time::sleep(Duration::from_secs(5)).await;
    }
  })
}

/// Core listen loop: connects, subscribes, and dispatches notifications.
async fn listen_loop(
  database_url: &str,
  channels: &[String],
  wakeup: &Notify,
) -> Result<(), tokio_postgres::Error> {
  let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<()>();
  let config = circus_migrations::tls::tokio_postgres_url(database_url)
    .parse::<tokio_postgres::Config>()?;

  let (client, driver) = match crate::db::tls_mode(database_url) {
    TlsMode::Disable => {
      let (client, conn) = config.connect(NoTls).await?;
      let driver = spawn_driver(conn, tx);
      subscribe(&client, channels).await?;
      (client, driver)
    },
    mode => {
      let connector = circus_migrations::tls::tls_connector(mode);
      let (client, conn) = config.connect(connector).await?;
      let driver = spawn_driver(conn, tx);
      subscribe(&client, channels).await?;
      (client, driver)
    },
  };

  tracing::info!(channels = ?channels, "PG LISTEN subscribed");

  while rx.recv().await.is_some() {
    // notify_one deposits a permit so notifications arriving while the daemon
    // is busy aren't lost.
    wakeup.notify_one();
  }

  drop(client);
  driver.abort();
  Ok(())
}

fn spawn_driver<S>(
  mut connection: tokio_postgres::Connection<tokio_postgres::Socket, S>,
  tx: tokio::sync::mpsc::UnboundedSender<()>,
) -> JoinHandle<()>
where
  S: tokio_postgres::tls::TlsStream + Unpin + Send + 'static,
{
  tokio::spawn(async move {
    let mut messages = stream::poll_fn(move |cx| connection.poll_message(cx));
    while let Some(msg) = messages.next().await {
      match msg {
        Ok(AsyncMessage::Notification(_)) => {
          if tx.send(()).is_err() {
            break;
          }
        },
        Ok(_) => {},
        Err(_) => break,
      }
    }
  })
}

async fn subscribe(
  client: &tokio_postgres::Client,
  channels: &[String],
) -> Result<(), tokio_postgres::Error> {
  for channel in channels {
    let quoted = channel.replace('"', "\"\"");
    client
      .batch_execute(&format!("LISTEN \"{quoted}\""))
      .await?;
  }
  Ok(())
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "Fine in tests")]
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

  #[tokio::test]
  async fn listener_receives_notifications() {
    let Ok(url) = std::env::var("TEST_DATABASE_URL") else {
      return;
    };
    circus_migrations::run_migrations(&url)
      .await
      .expect("migrate test database");

    let wakeup = Arc::new(Notify::new());
    let channel = "circus_pg_notify_test";
    let listener = spawn_listener(&url, &[channel], Arc::clone(&wakeup));
    let pool = crate::db::build_pool(&url, 1).expect("build notifier pool");

    let received = tokio::time::timeout(Duration::from_secs(2), async {
      loop {
        notify(&pool, channel).await.expect("send notification");
        if tokio::time::timeout(Duration::from_millis(50), wakeup.notified())
          .await
          .is_ok()
        {
          break;
        }
      }
    })
    .await
    .is_ok();

    listener.abort();
    assert!(received, "listener did not receive a notification");
  }
}
