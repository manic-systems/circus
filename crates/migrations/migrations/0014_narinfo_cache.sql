-- Narinfo cache.
--
-- Records the narinfo metadata for every store path the runner has
-- successfully accepted into its cache via the agent's presigned
-- upload path. The SSH dispatch path continues to push via
-- `nix copy --to s3://...` and does not write here.
--
-- The cache route on circus-server reads this table to serve narinfo
-- responses without re-querying the agent or the underlying store. Rows
-- never expire; GC removes them when the matching store path falls off
-- the cache.
CREATE TABLE IF NOT EXISTS narinfo_cache (
  store_path TEXT PRIMARY KEY,
  -- sha256 of the uncompressed NAR, encoded as `sha256:<base32-or-hex>`.
  nar_hash TEXT NOT NULL,
  nar_size BIGINT NOT NULL,
  -- sha256 of the compressed file on object storage. Same encoding as
  -- nar_hash. Set when `compression != 'none'`, null otherwise.
  file_hash TEXT,
  file_size BIGINT,
  -- Advertised compression. Reuses the strings Nix understands:
  -- 'none', 'xz', 'bzip2', 'zstd', 'gzip', 'brotli'.
  compression TEXT NOT NULL DEFAULT 'none',
  -- URL relative to the cache root (e.g. `nar/<hash>.nar.zst`).
  url TEXT NOT NULL,
  deriver TEXT,
  "references" TEXT[] NOT NULL DEFAULT '{}',
  -- Signature minted by the runner if signing is configured.
  sig TEXT,
  -- Content-addressed reference (`fixed:...` or empty).
  ca TEXT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_narinfo_cache_created_at ON narinfo_cache (created_at);
