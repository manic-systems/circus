pub use circus_codegen::client::GenericClient;
pub use circus_migrations::tls::connect_once;
use color_eyre::eyre::Context as _;
use deadpool_postgres::Timeouts;
use tokio_postgres::NoTls;

pub type PgPool = deadpool_postgres::Pool;

pub type DbClient = deadpool_postgres::Client;

/// Generated queries bind to this like any plain client, so repos can offer
/// `*_in_transaction` variants.
pub type DbTransaction<'a> = deadpool_postgres::Transaction<'a>;

/// Build a deadpool-postgres pool, honoring the URL's `sslmode`.
///
/// # Errors
///
/// Returns an error if the pool configuration is invalid.
pub fn build_pool(
  database_url: &str,
  max_size: usize,
) -> color_eyre::Result<PgPool> {
  build_pool_with_timeouts(database_url, max_size, Timeouts::default())
}

/// Build a pool with explicit acquire and connection-creation timeouts.
///
/// # Errors
///
/// Returns an error if the pool configuration is invalid.
pub fn build_pool_with_timeouts(
  database_url: &str,
  max_size: usize,
  timeouts: Timeouts,
) -> color_eyre::Result<PgPool> {
  use deadpool_postgres::{
    Config,
    ManagerConfig,
    PoolConfig,
    RecyclingMethod,
    Runtime,
  };

  let mut cfg = Config::new();
  let target = circus_migrations::tls::resolve(database_url)?;
  cfg.url = Some(target.url);
  cfg.manager = Some(ManagerConfig {
    recycling_method: RecyclingMethod::Fast,
  });
  cfg.pool = Some(PoolConfig {
    max_size,
    timeouts,
    ..Default::default()
  });

  let pool = target
    .tls
    .map_or_else(
      || cfg.create_pool(Some(Runtime::Tokio1), NoTls),
      |connector| cfg.create_pool(Some(Runtime::Tokio1), connector),
    )
    .context("building postgres connection pool")?;

  Ok(pool)
}

#[must_use]
pub fn is_unique_violation(err: &tokio_postgres::Error) -> bool {
  err.as_db_error().is_some_and(|db| {
    *db.code() == tokio_postgres::error::SqlState::UNIQUE_VIOLATION
  })
}
