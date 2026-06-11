-- Ephemeral build agents (e.g. GitHub Actions runners) register for one
-- short-lived session and never reconnect. Mark them so the runner can prune
-- stale rows, and record how the agent authenticated.
ALTER TABLE builder_sessions
ADD COLUMN IF NOT EXISTS ephemeral BOOLEAN NOT NULL DEFAULT FALSE,
ADD COLUMN IF NOT EXISTS auth_kind TEXT NOT NULL DEFAULT 'token';

-- Supports the periodic prune of disconnected ephemeral sessions.
CREATE INDEX IF NOT EXISTS idx_builder_sessions_ephemeral ON builder_sessions (ephemeral, connected, last_seen);
