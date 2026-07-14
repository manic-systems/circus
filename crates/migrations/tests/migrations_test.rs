//! Integration tests for the migrations crate.
//!
//! Tests that need a live `PostgreSQL` skip themselves cleanly when no database
//! is reachable. Set `CIRCUS_TEST_DATABASE_URL` to point them somewhere, the
//! default suits the `nix develop` postgres dev shell. Each test derives its
//! own database name so runs can't clobber each other.
#![expect(clippy::expect_used, clippy::print_stderr, reason = "Fine in tests")]

use circus_migrations::{
  REQUIRED_TABLES,
  REQUIRED_VIEWS,
  migration_set,
  run_migrations,
  run_migrations_up_to,
  tls::connect_once,
  validate_schema,
};

fn test_database_url() -> String {
  use std::sync::atomic::{AtomicU32, Ordering};
  static COUNTER: AtomicU32 = AtomicU32::new(0);
  let n = COUNTER.fetch_add(1, Ordering::Relaxed);
  let url = std::env::var("CIRCUS_TEST_DATABASE_URL").unwrap_or_else(|_| {
    "postgresql://postgres:password@localhost/circus_migrations_test"
      .to_string()
  });
  let mut parsed = url::Url::parse(&url).expect("parse database URL");
  let dbname = parsed.path().trim_start_matches('/').to_owned();
  parsed.set_path(&format!("/{dbname}_{}_{n}", std::process::id()));
  parsed.to_string()
}

fn maintenance_url_and_dbname(url: &str) -> (String, String) {
  let mut parsed = url::Url::parse(url).expect("parse database URL");
  let dbname = parsed.path().trim_start_matches('/').to_owned();
  parsed.set_path(if dbname == "postgres" {
    "/template1"
  } else {
    "/postgres"
  });
  (parsed.to_string(), dbname)
}

async fn database_exists(url: &str) -> Result<bool, tokio_postgres::Error> {
  let (admin_url, dbname) = maintenance_url_and_dbname(url);
  let client = connect_once(&admin_url).await?;
  let exists = client
    .query_opt("SELECT 1 FROM pg_database WHERE datname = $1", &[&dbname])
    .await?
    .is_some();
  Ok(exists)
}

/// Failure to drop is non-fatal, a dead server gets detected right after and
/// the test skips.
async fn reset_database(url: &str) {
  let (admin_url, dbname) = maintenance_url_and_dbname(url);
  let Ok(client) = connect_once(&admin_url).await else {
    return;
  };
  let quoted = dbname.replace('"', "\"\"");
  let _ = client
    .batch_execute(&format!(
      "DROP DATABASE IF EXISTS \"{quoted}\" WITH (FORCE)"
    ))
    .await;
}

/// Try to verify the server is reachable. Returns `None` (skip) if not.
async fn require_postgres(url: &str) -> Option<()> {
  match database_exists(url).await {
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

  let client = connect_once(&url).await.expect("connect after migrate");

  validate_schema(&client).await.expect("validate_schema");

  for table in REQUIRED_TABLES {
    let row = client
      .query_one(
        "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = $1 \
         AND table_schema = 'public'",
        &[table],
      )
      .await
      .expect("count tables");
    let n: i64 = row.get(0);
    assert_eq!(n, 1, "missing required table {table}");
  }
  for view in REQUIRED_VIEWS {
    let row = client
      .query_one(
        "SELECT COUNT(*) FROM information_schema.views WHERE table_name = $1 \
         AND table_schema = 'public'",
        &[view],
      )
      .await
      .expect("count views");
    let n: i64 = row.get(0);
    assert_eq!(n, 1, "missing required view {view}");
  }
}

#[tokio::test]
async fn migrations_are_idempotent_when_run_twice() {
  let url = test_database_url();
  let Some(()) = require_postgres(&url).await else {
    return;
  };

  reset_database(&url).await;

  run_migrations(&url).await.expect("first run");
  run_migrations(&url).await.expect("second run");

  let client = connect_once(&url).await.expect("connect");
  validate_schema(&client)
    .await
    .expect("validate after replay");

  let applied: i64 = client
    .query_one("SELECT COUNT(*) FROM _sqlx_migrations", &[])
    .await
    .expect("count applied")
    .get(0);
  assert_eq!(
    applied as usize,
    migration_set().len(),
    "applied count does not match static migration set"
  );

  let description: String = client
    .query_one(
      "SELECT description FROM _sqlx_migrations WHERE version = 1",
      &[],
    )
    .await
    .expect("read description")
    .get(0);
  assert_eq!(description, "tables");
}

#[tokio::test]
async fn run_migrations_creates_database_if_missing() {
  let url = test_database_url();
  let Some(()) = require_postgres(&url).await else {
    return;
  };

  reset_database(&url).await;
  assert!(
    !database_exists(&url).await.expect("exists check"),
    "precondition: db should not exist"
  );

  run_migrations(&url).await.expect("run on missing db");

  assert!(
    database_exists(&url).await.expect("exists check"),
    "run_migrations did not create the database"
  );
}

#[tokio::test]
async fn concurrent_runs_create_and_migrate_once() {
  let url = test_database_url();
  let Some(()) = require_postgres(&url).await else {
    return;
  };

  reset_database(&url).await;

  let results = tokio::join!(
    run_migrations(&url),
    run_migrations(&url),
    run_migrations(&url),
    run_migrations(&url),
  );
  results.0.expect("concurrent migration 1");
  results.1.expect("concurrent migration 2");
  results.2.expect("concurrent migration 3");
  results.3.expect("concurrent migration 4");

  let client = connect_once(&url).await.expect("connect");
  let applied: i64 = client
    .query_one("SELECT COUNT(*) FROM _sqlx_migrations", &[])
    .await
    .expect("count migrations")
    .get(0);
  assert_eq!(applied as usize, migration_set().len());
}

#[tokio::test]
async fn rejects_applied_migration_missing_from_binary() {
  let url = test_database_url();
  let Some(()) = require_postgres(&url).await else {
    return;
  };

  reset_database(&url).await;
  run_migrations(&url).await.expect("migrate");

  let client = connect_once(&url).await.expect("connect");
  client
    .execute(
      "INSERT INTO _sqlx_migrations
         (version, description, success, checksum, execution_time)
       VALUES ($1, $2, TRUE, $3, 0)",
      &[&999_i64, &"future migration", &vec![0_u8; 48]],
    )
    .await
    .expect("insert unknown migration");

  let error = run_migrations(&url)
    .await
    .expect_err("unknown migration must fail");
  assert!(
    error
      .to_string()
      .contains("applied migration 999 is missing from this binary"),
    "unexpected error: {error:#}"
  );
}

#[tokio::test]
async fn can_upgrade_existing_database_from_previous_migration() {
  let url = test_database_url();
  let Some(()) = require_postgres(&url).await else {
    return;
  };

  reset_database(&url).await;

  // Version 25 is the last schema before active_jobsets is rewritten.
  run_migrations_up_to(&url, 25)
    .await
    .expect("run previous migrations");

  run_migrations(&url).await.expect("upgrade to latest");

  let client = connect_once(&url).await.expect("connect after upgrade");
  validate_schema(&client)
    .await
    .expect("validate upgraded schema");

  let rows = client
    .query(
      "SELECT column_name FROM information_schema.columns WHERE table_schema \
       = 'public' AND table_name = 'active_jobsets' ORDER BY ordinal_position",
      &[],
    )
    .await
    .expect("active_jobsets columns");
  let active_jobset_columns: Vec<String> =
    rows.iter().map(|row| row.get(0)).collect();

  assert_eq!(active_jobset_columns, [
    "id",
    "project_id",
    "name",
    "nix_expression",
    "enabled",
    "flake_mode",
    "check_interval",
    "branch",
    "branch_pattern",
    "tag_pattern",
    "scheduling_shares",
    "created_at",
    "updated_at",
    "state",
    "last_checked_at",
    "keep_nr",
    "project_name",
    "repository_url",
    "trigger_mode",
  ]);
}

#[test]
fn migration_set_is_non_empty_and_strictly_increasing() {
  let set = migration_set();
  assert!(!set.is_empty(), "no migrations registered at compile time");

  let mut prev = i64::MIN;
  for (version, name) in &set {
    assert!(
      *version > prev,
      "migration versions must be strictly increasing, saw {version} ({name}) \
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
