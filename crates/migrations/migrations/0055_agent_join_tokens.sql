-- Agents that join with a shared token get a runner-minted machine ID and
-- session token, whose SHA-256 is stored here. A revoked identity keeps its
-- row for build history but frees its name.
ALTER TABLE builder_sessions
DROP COLUMN IF EXISTS auth_token_hash,
ADD COLUMN IF NOT EXISTS session_token_hash BYTEA UNIQUE,
ADD COLUMN IF NOT EXISTS join_token TEXT,
ADD COLUMN IF NOT EXISTS revoked_at TIMESTAMPTZ;

ALTER TABLE builder_sessions
DROP CONSTRAINT IF EXISTS builder_sessions_name_key;

CREATE UNIQUE INDEX IF NOT EXISTS idx_builder_sessions_active_name ON builder_sessions (name)
WHERE
  revoked_at IS NULL;
