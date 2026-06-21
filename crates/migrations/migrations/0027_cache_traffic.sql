-- Cache observability: per-NAR last-fetched tracking and aggregate serving
-- traffic. `narinfo_cache` already carries `project_id`, so storage stats group
-- by that and map to a derived cache name ('global' vs project name) rather than
-- stamping a redundant cache_name on every narinfo row.
ALTER TABLE narinfo_cache
ADD COLUMN IF NOT EXISTS last_fetched_at TIMESTAMPTZ;

CREATE TABLE cache_traffic (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  cache_name TEXT NOT NULL, -- 'global' or project name
  recorded_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  requests BIGINT NOT NULL DEFAULT 0,
  bytes_served BIGINT NOT NULL DEFAULT 0
);

CREATE INDEX idx_cache_traffic_name_time ON cache_traffic (cache_name, recorded_at DESC);

CREATE INDEX idx_narinfo_cache_project ON narinfo_cache (project_id);
