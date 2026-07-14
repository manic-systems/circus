-- Migration runner state. The `_sqlx_migrations` table comes from
-- `crates/migrations/bootstrap.sql`, which the codegen scripts apply to the
-- scratch database so these queries type-check.

--! advisory_lock
SELECT true AS locked FROM pg_advisory_lock(:key);

--! advisory_unlock
SELECT pg_advisory_unlock(:key);

--! database_exists
SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = :dbname) AS present;

--! current_database
SELECT current_database()::text AS name;

--: AppliedMigrationRow()

--! applied_migrations : AppliedMigrationRow
SELECT version, checksum, success FROM _sqlx_migrations ORDER BY version;

--! record_applied
INSERT INTO _sqlx_migrations (
  version, description, success, checksum, execution_time
)
VALUES (:version, :description, TRUE, :checksum, :execution_time);

--! table_exists
SELECT EXISTS(
  SELECT 1 FROM information_schema.tables
  WHERE table_name::text = :name AND table_schema = 'public'
) AS present;

--! view_exists
SELECT EXISTS(
  SELECT 1 FROM information_schema.views
  WHERE table_name::text = :name AND table_schema = 'public'
) AS present;
