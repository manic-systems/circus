//! Migration runner compatible with the existing `_sqlx_migrations` history.

use std::{collections::HashSet, time::Instant};

use circus_codegen::queries::migrations as q;
use color_eyre::eyre::{Context, eyre};
use crc::{CRC_32_ISO_HDLC, Crc};
use sha2::{Digest, Sha384};
use tracing::{info, warn};

pub mod tls;

struct Migration {
  version: i64,
  name:    &'static str,
  sql:     &'static str,
}

macro_rules! migrations {
  ($(($version:expr, $name:literal)),+ $(,)?) => {
    &[$(Migration {
      version: $version,
      name: $name,
      sql: include_str!(concat!("../migrations/", $name, ".sql")),
    }),+]
  };
}

const MIGRATIONS: &[Migration] = migrations![
  (1, "0001_tables"),
  (2, "0002_indexes"),
  (3, "0003_triggers"),
  (4, "0004_views"),
  (5, "0005_builder_cpu_cores"),
  (6, "0006_active_jobsets_enabled_filter"),
  (7, "0007_news_and_fod"),
  (8, "0008_service_heartbeats"),
  (9, "0009_audit_log"),
  (10, "0010_evaluations_notify"),
  (11, "0011_build_meta"),
  (12, "0012_builder_sessions"),
  (13, "0013_build_required_features"),
  (14, "0014_narinfo_cache"),
  (15, "0015_jobset_trigger_modes"),
  (16, "0016_evaluation_visibility"),
  (17, "0017_narinfo_cache_url_index"),
  (18, "0018_build_stats_include_pending"),
  (19, "0019_remove_build_log_url"),
  (20, "0020_build_agent"),
  (21, "0021_build_started_notified_at"),
  (22, "0022_build_effective_features"),
  (23, "0023_ephemeral_builder_sessions"),
  (24, "0024_notification_configs_slack"),
  (25, "0025_project_caches"),
  (26, "0026_jobset_ref_patterns"),
  (27, "0027_cache_traffic"),
  (28, "0028_narinfo_cache_project_owners"),
  (29, "0029_evaluation_cancellation"),
  (30, "0030_runtime_enum_constraints"),
  (31, "0031_evaluation_started_at"),
];

/// Runs all migrations, creating the database first if it doesn't exist.
///
/// # Errors
///
/// Returns an error if the database can't be created or reached, a migration
/// fails, or an already-applied migration's file changed.
pub async fn run_migrations(database_url: &str) -> color_eyre::Result<()> {
  run_migrations_up_to(database_url, i64::MAX).await
}

/// Like [`run_migrations`] but stops after `max_version`, so tests can build
/// an older schema and exercise the upgrade path.
///
/// # Errors
///
/// Returns an error if the database can't be created or reached, a migration
/// fails, or an already-applied migration's file changed.
pub async fn run_migrations_up_to(
  database_url: &str,
  max_version: i64,
) -> color_eyre::Result<()> {
  info!("Starting database migrations");
  ensure_database_exists(database_url).await?;

  let mut client = tls::connect_once(database_url)
    .await
    .context("connecting to database for migrations")?;

  let lock_key = migration_lock_key(&client).await?;
  q::advisory_lock()
    .bind(&client, &lock_key)
    .one()
    .await
    .context("acquiring migration advisory lock")?;

  let result = async {
    client
      .batch_execute(include_str!("../bootstrap.sql"))
      .await
      .context("creating _sqlx_migrations table")?;

    apply_pending(&mut client, max_version).await
  }
  .await;

  let unlock = q::advisory_unlock().bind(&client, &lock_key).one().await;

  result?;
  unlock.context("releasing migration advisory lock")?;
  info!("Database migrations completed successfully");
  Ok(())
}

async fn ensure_database_exists(database_url: &str) -> color_eyre::Result<()> {
  let mut url =
    url::Url::parse(database_url).context("parsing database URL")?;
  let dbname = urlencoding::decode(url.path().trim_start_matches('/'))
    .context("decoding database name")?
    .into_owned();
  if dbname.is_empty() {
    return Err(eyre!("database URL has no database name"));
  }

  // When the target is postgres itself the only maintenance DB left is
  // template1.
  url.set_path(if dbname == "postgres" {
    "/template1"
  } else {
    "/postgres"
  });
  let admin_url = url.to_string();
  let client = tls::connect_once(&admin_url)
    .await
    .context("connecting to maintenance database")?;

  let exists = q::database_exists()
    .bind(&client, &dbname)
    .one()
    .await
    .context("checking whether database exists")?;

  if !exists {
    warn!(database = %dbname, "database does not exist, creating it");
    // Identifiers can't be bound as parameters, so quote the name instead.
    let quoted = dbname.replace('"', "\"\"");
    match client
      .batch_execute(&format!("CREATE DATABASE \"{quoted}\""))
      .await
    {
      Ok(()) => info!(database = %dbname, "database created"),
      Err(err) => {
        // Every service runs this on startup, losing the CREATE race to a
        // peer is fine.
        let created_by_peer = q::database_exists()
          .bind(&client, &dbname)
          .one()
          .await
          .context("rechecking database after create failed")?;
        if !created_by_peer {
          return Err(err).context("creating database");
        }
      },
    }
  }
  Ok(())
}

async fn migration_lock_key(
  client: &tokio_postgres::Client,
) -> color_eyre::Result<i64> {
  const CRC_IEEE: Crc<u32> = Crc::<u32>::new(&CRC_32_ISO_HDLC);

  let database = q::current_database()
    .bind(client)
    .one()
    .await
    .context("reading current database")?;

  // Match SQLx so old and new binaries serialize rolling-upgrade migrations.
  Ok(0x3D32_AD9E * i64::from(CRC_IEEE.checksum(database.as_bytes())))
}

async fn apply_pending(
  client: &mut tokio_postgres::Client,
  max_version: i64,
) -> color_eyre::Result<()> {
  let applied = validate_applied(client).await?;
  for migration in MIGRATIONS {
    if migration.version > max_version {
      break;
    }
    if applied.contains(&migration.version) {
      continue;
    }

    let checksum = migration_checksum(migration);
    apply_one(client, migration, &checksum).await?;
  }
  Ok(())
}

async fn validate_applied(
  client: &tokio_postgres::Client,
) -> color_eyre::Result<HashSet<i64>> {
  let rows = q::applied_migrations()
    .bind(client)
    .all()
    .await
    .context("reading migration state")?;
  let mut applied = HashSet::with_capacity(rows.len());

  for row in rows {
    let version = row.version;
    let Some(migration) = MIGRATIONS.iter().find(|m| m.version == version)
    else {
      return Err(eyre!(
        "applied migration {version} is missing from this binary"
      ));
    };
    if !row.success {
      return Err(eyre!(
        "migration {version} previously failed and needs manual intervention"
      ));
    }
    if row.checksum != migration_checksum(migration) {
      return Err(eyre!(
        "migration {version} checksum mismatch, the file changed after it was \
         applied"
      ));
    }
    applied.insert(version);
  }

  Ok(applied)
}

fn migration_checksum(migration: &Migration) -> Vec<u8> {
  Sha384::digest(migration.sql.as_bytes()).to_vec()
}

async fn apply_one(
  client: &mut tokio_postgres::Client,
  migration: &Migration,
  checksum: &[u8],
) -> color_eyre::Result<()> {
  let started = Instant::now();
  let tx = client
    .transaction()
    .await
    .context("starting migration transaction")?;
  tx.batch_execute(migration.sql)
    .await
    .with_context(|| format!("applying migration {}", migration.version))?;
  let execution_time =
    i64::try_from(started.elapsed().as_nanos()).unwrap_or(i64::MAX);
  // sqlx stored descriptions without the version prefix and with spaces,
  // keep new rows matching.
  let description = migration
    .name
    .split_once('_')
    .map_or(migration.name, |(_, description)| description)
    .replace('_', " ");
  q::record_applied()
    .bind(
      &tx,
      &migration.version,
      &description,
      &checksum,
      &execution_time,
    )
    .await
    .context("recording applied migration")?;
  tx.commit().await.context("committing migration")?;

  info!(
    version = migration.version,
    name = migration.name,
    "applied migration"
  );
  Ok(())
}

/// Validates that all required tables and views exist.
///
/// # Errors
///
/// Returns an error if a query fails or a required table or view is missing.
pub async fn validate_schema(
  client: &tokio_postgres::Client,
) -> color_eyre::Result<()> {
  info!("Validating database schema");

  for table in REQUIRED_TABLES {
    if !q::table_exists().bind(client, table).one().await? {
      return Err(eyre!("Required table '{table}' does not exist"));
    }
  }

  for view in REQUIRED_VIEWS {
    if !q::view_exists().bind(client, view).one().await? {
      return Err(eyre!("Required view '{view}' does not exist"));
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
  "builder_sessions",
  "builds",
  "cache_traffic",
  "channels",
  "evaluations",
  "failed_paths_cache",
  "jobset_inputs",
  "jobsets",
  "narinfo_cache",
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

/// Migration `(version, name)` pairs in application order.
#[must_use]
pub fn migration_set() -> Vec<(i64, String)> {
  MIGRATIONS
    .iter()
    .map(|m| (m.version, m.name.to_string()))
    .collect()
}

#[cfg(test)]
#[expect(clippy::expect_used, reason = "Fine in tests")]
mod tests {
  use super::*;

  #[test]
  fn migrations_list_matches_directory() {
    let dir =
      std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
    let mut on_disk = std::fs::read_dir(&dir)
      .expect("read migrations dir")
      .map(|entry| {
        entry
          .expect("dir entry")
          .file_name()
          .to_str()
          .expect("utf8 filename")
          .to_owned()
      })
      .filter_map(|name| {
        name
          .strip_suffix(".sql")
          .map(std::string::ToString::to_string)
      })
      .map(|stem| {
        let version = stem
          .split('_')
          .next()
          .and_then(|v| v.parse::<i64>().ok())
          .expect("numeric version prefix");
        (version, stem)
      })
      .collect::<Vec<(i64, String)>>();
    on_disk.sort();

    assert_eq!(
      migration_set(),
      on_disk,
      "MIGRATIONS must match the migrations directory exactly"
    );
  }
}
