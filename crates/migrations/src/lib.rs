//! Database migration utilities

use sqlx::{PgPool, Postgres, migrate::MigrateDatabase};
use tracing::{error, info, warn};

/// Runs database migrations and ensures the database exists
///
/// # Errors
///
/// Returns error if database operations or migrations fail.
pub async fn run_migrations(database_url: &str) -> color_eyre::Result<()> {
  info!("Starting database migrations");

  // Check if database exists, create if it doesn't
  if !Postgres::database_exists(database_url).await? {
    warn!("Database does not exist, creating it");
    Postgres::create_database(database_url).await?;
    info!("Database created successfully");
  }

  // Set up connection pool with retry logic, then run migrations
  let pool = create_connection_pool(database_url).await?;
  match sqlx::migrate!("./migrations").run(&pool).await {
    Ok(()) => {
      info!("Database migrations completed successfully");
      Ok(())
    },
    Err(e) => {
      let last_applied = last_applied_migration(&pool)
        .await
        .unwrap_or_else(|lookup_err| format!("unknown ({lookup_err})"));

      error!("Failed to run database migrations after {last_applied}: {e}");
      Err(color_eyre::eyre::eyre!(
        "Migration failed after {last_applied}: {e}"
      ))
    },
  }
}

/// Creates a connection pool with proper configuration
async fn create_connection_pool(
  database_url: &str,
) -> color_eyre::Result<PgPool> {
  let pool = PgPool::connect(database_url).await?;

  // Test the connection
  sqlx::query("SELECT 1").fetch_one(&pool).await?;

  Ok(pool)
}

async fn last_applied_migration(pool: &PgPool) -> color_eyre::Result<String> {
  let version = sqlx::query_scalar::<_, Option<i64>>(
    "SELECT MAX(version) FROM _sqlx_migrations WHERE success = true",
  )
  .fetch_one(pool)
  .await?;

  Ok(version.map_or_else(
    || "no successful migrations".to_string(),
    |version| format!("migration {version}"),
  ))
}

/// Validates that all required tables exist and have the expected structure
///
/// # Errors
///
/// Returns error if schema validation fails or required tables are missing.
pub async fn validate_schema(pool: &PgPool) -> color_eyre::Result<()> {
  info!("Validating database schema");

  for table in REQUIRED_TABLES {
    let result = sqlx::query_scalar::<_, i64>(
      "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = $1 \
       AND table_schema = 'public'",
    )
    .bind(table)
    .fetch_one(pool)
    .await?;

    if result == 0 {
      return Err(color_eyre::eyre::eyre!(
        "Required table '{table}' does not exist"
      ));
    }
  }

  for view in REQUIRED_VIEWS {
    let result = sqlx::query_scalar::<_, i64>(
      "SELECT COUNT(*) FROM information_schema.views WHERE table_name = $1 \
       AND table_schema = 'public'",
    )
    .bind(view)
    .fetch_one(pool)
    .await?;

    if result == 0 {
      return Err(color_eyre::eyre::eyre!(
        "Required view '{view}' does not exist"
      ));
    }
  }

  info!("Database schema validation passed");
  Ok(())
}

/// Tables every migrated database must contain. Kept in sync with the SQL in
/// `migrations/`.
pub const REQUIRED_TABLES: &[&str] = &[
  "api_keys",
  "audit_log",
  "build_dependencies",
  "build_metrics",
  "build_outputs",
  "build_products",
  "build_steps",
  "builds",
  "channels",
  "evaluations",
  "failed_paths_cache",
  "jobset_inputs",
  "jobsets",
  "news",
  "notification_configs",
  "notification_tasks",
  "project_members",
  "projects",
  "remote_builders",
  "service_heartbeats",
  "starred_jobs",
  "user_sessions",
  "users",
  "webhook_configs",
];

/// Views every migrated database must contain.
pub const REQUIRED_VIEWS: &[&str] =
  &["active_jobsets", "build_metrics_summary", "build_stats"];

/// Static migration descriptors exposed for inspection by tests and tooling.
///
/// # Returns
///
/// Returns `(version, name)` pairs in the order sqlx will apply them.
#[must_use]
pub fn migration_set() -> Vec<(i64, String)> {
  sqlx::migrate!("./migrations")
    .iter()
    .map(|m| (m.version, m.description.to_string()))
    .collect()
}
