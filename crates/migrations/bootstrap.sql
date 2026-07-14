-- The migration tracking table itself, applied by the runner before anything
-- else and by the codegen scripts so queries against it type-check. Not a
-- migration, it must exist before migration state can be read.
CREATE TABLE IF NOT EXISTS _sqlx_migrations (
  version BIGINT PRIMARY KEY,
  description TEXT NOT NULL,
  installed_on TIMESTAMPTZ NOT NULL DEFAULT now(),
  success BOOLEAN NOT NULL,
  checksum BYTEA NOT NULL,
  execution_time BIGINT NOT NULL
);
