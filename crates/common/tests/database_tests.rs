//! Database integration tests

use circus_common::{config::DatabaseConfig, *};
use sqlx::PgPool;

#[tokio::test]
async fn test_database_connection() -> anyhow::Result<()> {
  let config = DatabaseConfig {
    url:             "postgresql://postgres:password@localhost/test"
      .to_string(),
    max_connections: 5,
    min_connections: 1,
    connect_timeout: 5, // Short timeout for test
    idle_timeout:    600,
    max_lifetime:    1800,
  };

  // Try to connect, skip test if database is not available
  let db = match Database::new(config).await {
    Ok(db) => db,
    Err(e) => {
      println!(
        "Skipping test_database_connection: no PostgreSQL instance available \
         - {e}"
      );
      return Ok(());
    },
  };

  // Test health check
  Database::health_check(db.pool()).await?;

  // Test connection info
  let info = db.get_connection_info().await?;
  assert!(!info.database.is_empty());
  assert!(!info.user.is_empty());
  assert!(!info.version.is_empty());

  // Test pool stats
  let stats = db.get_pool_stats();
  assert!(stats.size >= 1);

  db.close().await;

  Ok(())
}

#[tokio::test]
async fn test_database_health_check() -> anyhow::Result<()> {
  // Try to connect, skip test if database is not available
  let pool = match PgPool::connect(
    "postgresql://postgres:password@localhost/test",
  )
  .await
  {
    Ok(pool) => pool,
    Err(e) => {
      println!(
        "Skipping test_database_health_check: no PostgreSQL instance \
         available - {e}"
      );
      return Ok(());
    },
  };

  // Should succeed
  Database::health_check(&pool).await?;

  pool.close().await;
  Ok(())
}

#[tokio::test]
async fn test_connection_info() -> anyhow::Result<()> {
  // Try to connect, skip test if database is not available
  let pool = match PgPool::connect(
    "postgresql://postgres:password@localhost/test",
  )
  .await
  {
    Ok(pool) => pool,
    Err(e) => {
      println!(
        "Skipping test_connection_info: no PostgreSQL instance available - {e}"
      );
      return Ok(());
    },
  };

  let db = match Database::new(DatabaseConfig {
    url:             "postgresql://postgres:password@localhost/test"
      .to_string(),
    max_connections: 5,
    min_connections: 1,
    connect_timeout: 5, // Short timeout for test
    idle_timeout:    600,
    max_lifetime:    1800,
  })
  .await
  {
    Ok(db) => db,
    Err(e) => {
      println!(
        "Skipping test_connection_info: database connection failed - {e}"
      );
      pool.close().await;
      return Ok(());
    },
  };

  let info = db.get_connection_info().await?;

  assert!(!info.database.is_empty());
  assert!(!info.user.is_empty());
  assert!(!info.version.is_empty());
  assert!(info.version.contains("PostgreSQL"));

  db.close().await;
  pool.close().await;

  Ok(())
}

#[tokio::test]
async fn test_pool_stats() -> anyhow::Result<()> {
  let db = match Database::new(DatabaseConfig {
    url:             "postgresql://postgres:password@localhost/test"
      .to_string(),
    max_connections: 5,
    min_connections: 1,
    connect_timeout: 5, // Short timeout for test
    idle_timeout:    600,
    max_lifetime:    1800,
  })
  .await
  {
    Ok(db) => db,
    Err(e) => {
      println!(
        "Skipping test_pool_stats: no PostgreSQL instance available - {e}"
      );
      return Ok(());
    },
  };

  let stats = db.get_pool_stats();

  assert!(stats.size >= 1);
  assert!(stats.idle >= 1);
  assert_eq!(stats.size, stats.idle + stats.active);

  db.close().await;

  Ok(())
}

#[sqlx::test]
async fn test_database_config_validation() -> anyhow::Result<()> {
  // Valid config
  let config = DatabaseConfig {
    url:             "postgresql://user:pass@localhost/db".to_string(),
    max_connections: 10,
    min_connections: 2,
    connect_timeout: 30,
    idle_timeout:    600,
    max_lifetime:    1800,
  };
  assert!(config.validate().is_ok());

  // Invalid URL
  let mut config = config;
  config.url = "invalid://url".to_string();
  assert!(config.validate().is_err());

  // Empty URL
  config.url = String::new();
  assert!(config.validate().is_err());

  // Zero max connections
  config = DatabaseConfig {
    url:             "postgresql://user:pass@localhost/db".to_string(),
    max_connections: 0,
    min_connections: 1,
    connect_timeout: 30,
    idle_timeout:    600,
    max_lifetime:    1800,
  };
  assert!(config.validate().is_err());

  // Min > max
  config = DatabaseConfig {
    url:             "postgresql://user:pass@localhost/db".to_string(),
    max_connections: 5,
    min_connections: 10,
    connect_timeout: 30,
    idle_timeout:    600,
    max_lifetime:    1800,
  };
  assert!(config.validate().is_err());

  Ok(())
}
