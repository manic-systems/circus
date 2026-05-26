//! Integration tests for database and configuration

mod notifications_tests;

use circus_common::{
  Database,
  config::{Config, DatabaseConfig},
};

#[tokio::test]
async fn test_database_connection_full() -> anyhow::Result<()> {
  // This test requires a running PostgreSQL instance
  // Skip if no database is available
  let config = DatabaseConfig {
    url:             "postgresql://postgres:password@localhost/circus_test"
      .to_string(),
    max_connections: 5,
    min_connections: 1,
    connect_timeout: 5, // Short timeout for test
    idle_timeout:    600,
    max_lifetime:    1800,
  };

  // Try to connect, skip test if database is not available
  let Ok(db) = Database::new(config).await else {
    println!("Skipping database test: no PostgreSQL instance available");
    return Ok(());
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
  assert!(stats.idle >= 1);
  assert_eq!(stats.size, stats.idle + stats.active);

  db.close().await;

  Ok(())
}

#[test]
fn test_config_loading() -> anyhow::Result<()> {
  // Test default config loading
  let config = Config::load()?;
  assert!(config.validate().is_ok());

  // Test that defaults are reasonable
  assert_eq!(config.database.max_connections, 20);
  assert_eq!(config.database.min_connections, 5);
  assert_eq!(config.server.port, 3000);
  assert_eq!(config.evaluator.poll_interval, 60);
  assert_eq!(config.queue_runner.workers, 4);

  Ok(())
}

#[test]
fn test_config_validation() -> anyhow::Result<()> {
  // Test valid config
  let base_config = Config::default();
  assert!(base_config.validate().is_ok());

  // Test invalid database URL
  let mut config = base_config.clone();
  config.database.url = "invalid://url".to_string();
  assert!(config.validate().is_err());

  // Test invalid port
  let mut config = base_config.clone();
  config.server.port = 0;
  assert!(config.validate().is_err());

  // Test invalid connections
  let mut config = base_config.clone();
  config.database.max_connections = 0;
  assert!(config.validate().is_err());

  config.database.max_connections = 10;
  config.database.min_connections = 15;
  assert!(config.validate().is_err());

  // Test invalid evaluator settings
  let mut config = base_config.clone();
  config.evaluator.poll_interval = 0;
  assert!(config.validate().is_err());

  // Test invalid queue runner settings
  let mut config = base_config;
  config.queue_runner.workers = 0;
  assert!(config.validate().is_err());

  Ok(())
}

#[test]
fn test_database_config_validation() -> anyhow::Result<()> {
  // Test valid config
  let config = DatabaseConfig::default();
  assert!(config.validate().is_ok());

  // Test invalid URL
  let mut config = config;
  config.url = "invalid://url".to_string();
  assert!(config.validate().is_err());

  // Test empty URL
  config.url = String::new();
  assert!(config.validate().is_err());

  // Test zero max connections
  config = DatabaseConfig::default();
  config.max_connections = 0;
  assert!(config.validate().is_err());

  // Test min > max
  config = DatabaseConfig::default();
  config.max_connections = 5;
  config.min_connections = 10;
  assert!(config.validate().is_err());

  Ok(())
}

#[test]
fn test_config_serialization() -> anyhow::Result<()> {
  let config = Config::default();

  // Test TOML serialization
  let toml_str = toml::to_string_pretty(&config)?;
  let parsed: Config = toml::from_str(&toml_str)?;
  assert_eq!(config.database.url, parsed.database.url);
  assert_eq!(config.server.port, parsed.server.port);

  // Test JSON serialization
  let json_str = serde_json::to_string_pretty(&config)?;
  let parsed: Config = serde_json::from_str(&json_str)?;
  assert_eq!(config.database.url, parsed.database.url);
  assert_eq!(config.server.port, parsed.server.port);

  Ok(())
}
