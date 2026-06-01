-- Fast lookup from a NAR URL advertised in narinfo to the persisted upload row.
-- Multiple store paths may share one NAR URL when their contents are identical,
-- so this index is intentionally non-unique.
CREATE INDEX IF NOT EXISTS idx_narinfo_cache_url ON narinfo_cache (url);
