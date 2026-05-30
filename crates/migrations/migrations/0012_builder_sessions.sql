-- Persistent record of a connected agent and its most recent heartbeat.
-- Rows are upserted by name on register, updated by heartbeats, and read by
-- the admin UI. The in-memory `AgentPool` is the source of truth for "is
-- this agent connected right now"; this table is the durable log so the UI
-- can show stats across restarts and so we have history for audit.
CREATE TABLE IF NOT EXISTS builder_sessions (
  -- Stable identity issued by the agent on first start, kept across
  -- reconnects. UUIDv4.
  machine_id UUID PRIMARY KEY,
  -- Operator-assigned name. Unique to give the dashboard a stable label.
  name TEXT NOT NULL UNIQUE,
  hostname TEXT NOT NULL,
  systems TEXT[] NOT NULL DEFAULT '{}',
  supported_features TEXT[] NOT NULL DEFAULT '{}',
  mandatory_features TEXT[] NOT NULL DEFAULT '{}',
  speed_factor REAL NOT NULL DEFAULT 1.0,
  cpu_count INTEGER NOT NULL DEFAULT 1,
  max_jobs INTEGER NOT NULL DEFAULT 1,
  proto_version TEXT NOT NULL,
  -- Last heartbeat snapshot. Nullable until the first heartbeat lands.
  last_seen TIMESTAMPTZ,
  current_jobs INTEGER NOT NULL DEFAULT 0,
  load1 REAL,
  load5 REAL,
  load15 REAL,
  mem_total BIGINT,
  mem_used BIGINT,
  store_free BIGINT,
  build_dir_free BIGINT,
  cpu_psi_avg10 REAL,
  mem_psi_avg10 REAL,
  io_psi_avg10 REAL,
  -- Set on register, cleared on graceful shutdown. Used by the admin UI
  -- to distinguish "agent is down" from "agent never registered".
  connected BOOLEAN NOT NULL DEFAULT TRUE,
  -- Per-agent counters carried across reconnects.
  builds_succeeded BIGINT NOT NULL DEFAULT 0,
  builds_failed BIGINT NOT NULL DEFAULT 0,
  consecutive_failures INTEGER NOT NULL DEFAULT 0,
  disabled_until TIMESTAMPTZ,
  -- Bearer-token hash (sha256 hex) the agent must present to register.
  -- Provisioned out of band by the operator; null means "any valid token
  -- from [builder].auth_tokens".
  auth_token_hash TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_builder_sessions_connected ON builder_sessions (connected);

CREATE INDEX IF NOT EXISTS idx_builder_sessions_systems ON builder_sessions USING GIN (systems);
