//! Integration tests for the migrations crate.
//!
//! Tests that require a live `PostgreSQL` instance skip themselves cleanly when
//! no database is reachable, matching the pattern used in
//! `crates/common/tests/database_tests.rs`. The pure tests at the bottom of
//! this file run unconditionally.
//!
//! Set `CIRCUS_TEST_DATABASE_URL` to point tests at a specific database. The
//! default, `postgresql://postgres:password@localhost/circus_migrations_test`,
//! is suitable for the `nix develop` postgres dev shell.
#![expect(clippy::expect_used, clippy::print_stderr, reason = "Fine in tests")]

use circus_migrations::{
  REQUIRED_TABLES,
  REQUIRED_VIEWS,
  migration_set,
  run_migrations,
  validate_schema,
};
use sqlx::{PgPool, Postgres, migrate::MigrateDatabase};

fn test_database_url() -> String {
  std::env::var("CIRCUS_TEST_DATABASE_URL").unwrap_or_else(|_| {
    "postgresql://postgres:password@localhost/circus_migrations_test"
      .to_string()
  })
}

/// Drop the test database if it exists so each test run starts from a clean
/// state. Failure to drop is non-fatal -- if the connection cannot be made
/// the caller will detect that next and skip.
async fn reset_database(url: &str) {
  if matches!(Postgres::database_exists(url).await, Ok(true)) {
    let _ = Postgres::drop_database(url).await;
  }
}

/// Try to verify the server is reachable. Returns `None` (skip) if not.
async fn require_postgres(url: &str) -> Option<()> {
  // We can't `PgPool::connect` to a non-existent DB, so just check whether
  // the server answers at all by attempting an "exists" lookup.
  match Postgres::database_exists(url).await {
    Ok(_) => Some(()),
    Err(e) => {
      eprintln!("Skipping: no PostgreSQL reachable at {url}: {e}");
      None
    },
  }
}

#[tokio::test]
async fn migrations_create_required_tables_and_views() {
  let url = test_database_url();
  let Some(()) = require_postgres(&url).await else {
    return;
  };

  reset_database(&url).await;

  run_migrations(&url).await.expect("run_migrations");

  let pool = PgPool::connect(&url).await.expect("connect after migrate");

  validate_schema(&pool).await.expect("validate_schema");

  // Belt and braces: explicitly check every table/view we claim to require.
  for table in REQUIRED_TABLES {
    let n: i64 = sqlx::query_scalar(
      "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = $1 \
       AND table_schema = 'public'",
    )
    .bind(table)
    .fetch_one(&pool)
    .await
    .expect("count tables");
    assert_eq!(n, 1, "missing required table {table}");
  }
  for view in REQUIRED_VIEWS {
    let n: i64 = sqlx::query_scalar(
      "SELECT COUNT(*) FROM information_schema.views WHERE table_name = $1 \
       AND table_schema = 'public'",
    )
    .bind(view)
    .fetch_one(&pool)
    .await
    .expect("count views");
    assert_eq!(n, 1, "missing required view {view}");
  }

  pool.close().await;
}

#[tokio::test]
async fn migrations_are_idempotent_when_run_twice() {
  let url = test_database_url();
  let Some(()) = require_postgres(&url).await else {
    return;
  };

  reset_database(&url).await;

  run_migrations(&url).await.expect("first run");
  // The hot path: a no-op replay must succeed against the same schema.
  run_migrations(&url).await.expect("second run");

  let pool = PgPool::connect(&url).await.expect("connect");
  validate_schema(&pool).await.expect("validate after replay");

  // sqlx records applied migrations in _sqlx_migrations; row count must equal
  // the static migration set length.
  let applied: i64 =
    sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
      .fetch_one(&pool)
      .await
      .expect("count applied");
  assert_eq!(
    applied as usize,
    migration_set().len(),
    "applied count does not match static migration set"
  );

  pool.close().await;
}

#[tokio::test]
async fn run_migrations_creates_database_if_missing() {
  let url = test_database_url();
  let Some(()) = require_postgres(&url).await else {
    return;
  };

  reset_database(&url).await;
  assert!(
    !Postgres::database_exists(&url).await.expect("exists check"),
    "precondition: db should not exist"
  );

  run_migrations(&url).await.expect("run on missing db");

  assert!(
    Postgres::database_exists(&url).await.expect("exists check"),
    "run_migrations did not create the database"
  );
}

/// Pure tests below: no postgres required, run on every host.
#[test]
fn migration_set_is_non_empty_and_strictly_increasing() {
  let set = migration_set();
  assert!(!set.is_empty(), "no migrations registered at compile time");

  let mut prev = i64::MIN;
  for (version, name) in &set {
    assert!(
      *version > prev,
      "migration versions must be strictly increasing; saw {version} ({name}) \
       after {prev}"
    );
    assert!(!name.is_empty(), "migration {version} has empty name");
    prev = *version;
  }
}

#[test]
fn required_tables_constant_has_no_duplicates_and_is_sorted() {
  let mut copy: Vec<&&str> = REQUIRED_TABLES.iter().collect();
  copy.sort();
  copy.dedup();
  assert_eq!(
    copy.len(),
    REQUIRED_TABLES.len(),
    "REQUIRED_TABLES contains duplicates"
  );

  let mut sorted: Vec<&&str> = REQUIRED_TABLES.iter().collect();
  sorted.sort();
  let original: Vec<&&str> = REQUIRED_TABLES.iter().collect();
  assert_eq!(sorted, original, "REQUIRED_TABLES should be sorted");
}

#[test]
fn required_views_constant_has_no_duplicates() {
  let mut copy: Vec<&&str> = REQUIRED_VIEWS.iter().collect();
  copy.sort();
  copy.dedup();
  assert_eq!(
    copy.len(),
    REQUIRED_VIEWS.len(),
    "REQUIRED_VIEWS contains duplicates"
  );
}
