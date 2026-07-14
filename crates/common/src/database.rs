//! Database connection and pool management

use std::time::Duration;

use circus_codegen::queries::{database as database_q, health as health_q};
use circus_config::DatabaseConfig;
use tracing::{debug, info, warn};

use crate::db::{self, PgPool};

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

    // Deadpool has no equivalents for the other configured pool lifetimes.
    let connect_timeout = Duration::from_secs(config.connect_timeout);
    let pool = db::build_pool_with_timeouts(
      &config.url,
      config.max_connections as usize,
      deadpool_postgres::Timeouts {
        wait:    Some(connect_timeout),
        create:  Some(connect_timeout),
        recycle: None,
      },
    )?;

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

    let client = pool.get().await?;
    let result = health_q::check().bind(&client).one().await?;

    if result != 1 {
      return Err(color_eyre::eyre::eyre!(
        "Database health check failed: unexpected result"
      ));
    }

    debug!("Database health check passed");
    Ok(())
  }

  /// Prevent new connections from being checked out.
  pub fn close(&self) {
    info!("Closing database connection pool");
    self.pool.close();
  }

  /// Query database metadata (version, user, address).
  ///
  /// # Errors
  ///
  /// Returns error if query fails.
  pub async fn get_connection_info(
    &self,
  ) -> color_eyre::Result<ConnectionInfo> {
    let client = self.pool.get().await?;
    let row = database_q::connection_info().bind(&client).one().await?;

    Ok(ConnectionInfo {
      database:    row.database,
      user:        row.user,
      version:     row.version,
      server_ip:   row.server_ip,
      server_port: row.server_port,
    })
  }

  /// Get current connection pool statistics (size, idle, active).
  #[must_use]
  pub fn get_pool_stats(&self) -> PoolStats {
    let status = self.pool.status();
    let size = status.size as u32;
    let idle = u32::try_from(status.available).unwrap_or(0);
    PoolStats {
      size,
      idle,
      active: size.saturating_sub(idle),
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
    if !self.pool.is_closed() {
      warn!("Database connection pool dropped without explicit close");
    }
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
