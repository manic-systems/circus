//! CLI utility for database migrations

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::fmt::init;

const DEFAULT_MIGRATIONS_DIR: &str = "crates/migrations/migrations";

#[derive(Debug, Parser)]
#[command(name = "circus-migrate")]
#[command(about = "Database migration utility for circus")]
pub struct Cli {
  #[command(subcommand)]
  pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
  /// Run all pending migrations
  Up {
    /// Database connection URL
    database_url: String,
  },
  /// Validate the current schema
  Validate {
    /// Database connection URL
    database_url: String,
  },
  /// Create a new migration file
  Create {
    /// Migration name (`snake_case`, becomes part of the filename)
    #[arg(required = true)]
    name:       String,
    /// Directory to write the migration file into. Defaults to
    /// `crates/migrations/migrations` so the command stays useful when
    /// invoked from the workspace root.
    #[arg(long, default_value = DEFAULT_MIGRATIONS_DIR)]
    output_dir: PathBuf,
  },
}

/// Execute the CLI command.
///
/// # Errors
///
/// Returns error if command execution fails.
#[expect(
  clippy::print_stdout,
  reason = "CLI output is the primary user-facing interface"
)]
pub async fn run() -> anyhow::Result<()> {
  let cli = Cli::parse();

  // Initialize logging
  init();

  match cli.command {
    Commands::Up { database_url } => {
      info!("Running database migrations");
      crate::run_migrations(&database_url).await?;
      info!("Migrations completed successfully");
    },
    Commands::Validate { database_url } => {
      info!("Validating database schema");
      let pool = sqlx::PgPool::connect(&database_url).await?;
      crate::validate_schema(&pool).await?;
      info!("Schema validation passed");
    },
    Commands::Create { name, output_dir } => {
      let path = create_migration(&output_dir, &name)?;
      println!("Created migration file: {}", path.display());
    },
  }

  Ok(())
}

/// Validate that `name` is safe to use as part of a migration filename.
fn validate_name(name: &str) -> anyhow::Result<()> {
  if name.is_empty() {
    return Err(anyhow::anyhow!("Migration name must not be empty"));
  }
  if name.len() > 80 {
    return Err(anyhow::anyhow!(
      "Migration name too long ({} chars); keep it under 80",
      name.len()
    ));
  }
  let ok = name
    .chars()
    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
  if !ok {
    return Err(anyhow::anyhow!(
      "Migration name must contain only ASCII alphanumerics, '_' or '-'; got: \
       {name}"
    ));
  }
  Ok(())
}

/// Create a new migration file under `output_dir`. Returns the path written.
///
/// Pulled out of `run()` so tests can drive it against a tempdir without
/// touching the workspace tree.
///
/// # Errors
///
/// Returns error if the name is invalid, the directory cannot be created,
/// or writing the file fails.
pub fn create_migration(
  output_dir: &Path,
  name: &str,
) -> anyhow::Result<PathBuf> {
  use std::fs;

  use chrono::Utc;

  validate_name(name)?;

  fs::create_dir_all(output_dir)?;

  let now = Utc::now();
  let timestamp = now.format("%Y%m%d_%H%M%S");
  let filename = format!("{timestamp}_{name}.sql");
  let filepath = output_dir.join(&filename);

  let content = format!(
    "-- Migration: {}\n-- Created: {}\n\n-- Add your migration SQL here\n\n-- \
     Uncomment below for rollback SQL\n-- ROLLBACK;\n",
    name,
    now.to_rfc3339()
  );

  fs::write(&filepath, content)?;

  Ok(filepath)
}

#[cfg(test)]
#[expect(
  clippy::unwrap_used,
  clippy::expect_used,
  clippy::panic,
  reason = "Fine in tests"
)]
mod tests {
  use clap::Parser;

  use super::*;

  #[test]
  fn cli_parses_up_with_url() {
    let cli =
      Cli::try_parse_from(["circus-migrate", "up", "postgres://x/y"]).unwrap();
    match cli.command {
      Commands::Up { database_url } => {
        assert_eq!(database_url, "postgres://x/y");
      },
      _ => panic!("expected Up subcommand"),
    }
  }

  #[test]
  fn cli_parses_validate_with_url() {
    let cli =
      Cli::try_parse_from(["circus-migrate", "validate", "postgres://x/y"])
        .unwrap();
    assert!(matches!(cli.command, Commands::Validate { .. }));
  }

  #[test]
  fn cli_parses_create_with_default_output_dir() {
    let cli =
      Cli::try_parse_from(["circus-migrate", "create", "add_foo"]).unwrap();
    match cli.command {
      Commands::Create { name, output_dir } => {
        assert_eq!(name, "add_foo");
        assert_eq!(output_dir, PathBuf::from(DEFAULT_MIGRATIONS_DIR));
      },
      _ => panic!("expected Create subcommand"),
    }
  }

  #[test]
  fn cli_parses_create_with_custom_output_dir() {
    let cli = Cli::try_parse_from([
      "circus-migrate",
      "create",
      "add_foo",
      "--output-dir",
      "/tmp/mig",
    ])
    .unwrap();
    match cli.command {
      Commands::Create { output_dir, .. } => {
        assert_eq!(output_dir, PathBuf::from("/tmp/mig"));
      },
      _ => panic!("expected Create subcommand"),
    }
  }

  #[test]
  fn cli_rejects_missing_subcommand() {
    let err = Cli::try_parse_from(["circus-migrate"]).unwrap_err();
    assert!(!err.to_string().is_empty());
  }

  #[test]
  fn cli_rejects_up_without_url() {
    let err = Cli::try_parse_from(["circus-migrate", "up"]).unwrap_err();
    assert!(!err.to_string().is_empty());
  }

  #[test]
  fn validate_name_accepts_typical_names() {
    assert!(validate_name("add_foo").is_ok());
    assert!(validate_name("add-foo").is_ok());
    assert!(validate_name("AddFoo123").is_ok());
  }

  #[test]
  fn validate_name_rejects_bad_input() {
    assert!(validate_name("").is_err());
    assert!(validate_name("../escape").is_err());
    assert!(validate_name("name with spaces").is_err());
    assert!(validate_name("name/with/slash").is_err());
    assert!(validate_name(&"x".repeat(100)).is_err());
  }

  #[test]
  fn create_migration_writes_a_well_formed_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = create_migration(tmp.path(), "add_foo").expect("create");

    assert!(path.starts_with(tmp.path()));
    let filename = path
      .file_name()
      .and_then(|s| s.to_str())
      .expect("utf8 filename");
    assert!(
      filename.ends_with("_add_foo.sql"),
      "unexpected filename: {filename}"
    );
    assert!(filename.starts_with(|c: char| c.is_ascii_digit()));

    let body = std::fs::read_to_string(&path).expect("read");
    assert!(body.contains("-- Migration: add_foo"));
    assert!(body.contains("ROLLBACK"));
  }

  #[test]
  fn create_migration_creates_missing_directories() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let nested = tmp.path().join("a").join("b");
    let path = create_migration(&nested, "thing").expect("create");
    assert!(path.exists());
  }

  #[test]
  fn create_migration_rejects_invalid_name() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let res = create_migration(tmp.path(), "../escape");
    assert!(res.is_err());
  }
}
