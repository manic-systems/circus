//! Database connection and pool management

use std::time::Duration;

use circus_config::DatabaseConfig;
use sqlx::{PgPool, Row, postgres::PgPoolOptions};
use tracing::{debug, info, warn};

pub struct Database {
  pool: PgPool,
}

impl Database {
  /// Create a new database connection pool from config.
  ///
  /// # Errors
  ///
  /// Returns error if connection fails or health check fails.
  pub async fn new(config: DatabaseConfig) -> color_eyre::Result<Self> {
    info!("Initializing database connection pool");

    let pool = PgPoolOptions::new()
      .max_connections(config.max_connections)
      .min_connections(config.min_connections)
      .acquire_timeout(Duration::from_secs(config.connect_timeout))
      .idle_timeout(Duration::from_secs(config.idle_timeout))
      .max_lifetime(Duration::from_secs(config.max_lifetime))
      .connect(&config.url)
      .await?;

    // Test the connection
    Self::health_check(&pool).await?;

    info!("Database connection pool initialized successfully");

    Ok(Self { pool })
  }

  /// Get a reference to the underlying connection pool.
  #[must_use]
  pub const fn pool(&self) -> &PgPool {
    &self.pool
  }

  /// Run a simple query to verify the database is reachable.
  ///
  /// # Errors
  ///
  /// Returns error if query fails or returns unexpected result.
  pub async fn health_check(pool: &PgPool) -> color_eyre::Result<()> {
    debug!("Performing database health check");

    let result: i32 = sqlx::query_scalar("SELECT 1").fetch_one(pool).await?;

    if result != 1 {
      return Err(color_eyre::eyre::eyre!(
        "Database health check failed: unexpected result"
      ));
    }

    debug!("Database health check passed");
    Ok(())
  }

  /// Close the connection pool gracefully.
  pub async fn close(&self) {
    info!("Closing database connection pool");
    self.pool.close().await;
  }

  /// Query database metadata (version, user, address).
  ///
  /// # Errors
  ///
  /// Returns error if query fails.
  pub async fn get_connection_info(
    &self,
  ) -> color_eyre::Result<ConnectionInfo> {
    let row = sqlx::query(
      r"
            SELECT 
                current_database() as database,
                current_user as user,
                version() as version,
                inet_server_addr() as server_ip,
                inet_server_port() as server_port
            ",
    )
    .fetch_one(&self.pool)
    .await?;

    Ok(ConnectionInfo {
      database:    row.get("database"),
      user:        row.get("user"),
      version:     row.get("version"),
      server_ip:   row.get("server_ip"),
      server_port: row.get("server_port"),
    })
  }

  /// Get current connection pool statistics (size, idle, active).
  #[must_use]
  pub fn get_pool_stats(&self) -> PoolStats {
    let pool = &self.pool;

    PoolStats {
      size:   pool.size(),
      idle:   pool.num_idle() as u32,
      active: (pool.size() - pool.num_idle() as u32),
    }
  }
}

#[derive(Debug, Clone)]
pub struct ConnectionInfo {
  pub database:    String,
  pub user:        String,
  pub version:     String,
  pub server_ip:   Option<String>,
  pub server_port: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct PoolStats {
  pub size:   u32,
  pub idle:   u32,
  pub active: u32,
}

impl Drop for Database {
  fn drop(&mut self) {
    warn!("Database connection pool dropped without explicit close");
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_pool_stats() {
    let stats = PoolStats {
      size:   10,
      idle:   3,
      active: 7,
    };

    assert_eq!(stats.size, 10);
    assert_eq!(stats.idle, 3);
    assert_eq!(stats.active, 7);
  }

  #[test]
  fn test_connection_info() {
    let info = ConnectionInfo {
      database:    "test_db".to_string(),
      user:        "test_user".to_string(),
      version:     "PostgreSQL 14.0".to_string(),
      server_ip:   Some("127.0.0.1".to_string()),
      server_port: Some(5432),
    };

    assert_eq!(info.database, "test_db");
    assert_eq!(info.user, "test_user");
    assert_eq!(info.server_port, Some(5432));
  }
}
