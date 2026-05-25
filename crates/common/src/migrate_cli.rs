//! CLI utility for database migrations

use clap::{Parser, Subcommand};
use tracing::info;
use tracing_subscriber::fmt::init;

#[derive(Parser)]
#[command(name = "circus-migrate")]
#[command(about = "Database migration utility for circus")]
pub struct Cli {
  #[command(subcommand)]
  pub command: Commands,
}

#[derive(Subcommand)]
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
    /// Migration name
    #[arg(required = true)]
    name: String,
  },
}

/// Execute the CLI command.
///
/// # Errors
///
/// Returns error if command execution fails.
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
    Commands::Create { name } => {
      create_migration(&name)?;
    },
  }

  Ok(())
}

fn create_migration(name: &str) -> anyhow::Result<()> {
  use std::fs;

  use chrono::Utc;

  let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
  let filename = format!("{timestamp}_{name}.sql");
  let filepath = format!("crates/common/migrations/{filename}");

  let content = format!(
    "-- Migration: {}\n-- Created: {}\n\n-- Add your migration SQL here\n\n-- \
     Uncomment below for rollback SQL\n-- ROLLBACK;\n",
    name,
    Utc::now().to_rfc3339()
  );

  fs::write(&filepath, content)?;
  println!("Created migration file: {filepath}");

  Ok(())
}
