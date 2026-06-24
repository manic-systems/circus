CREATE TABLE IF NOT EXISTS narinfo_cache_projects (
  store_path TEXT NOT NULL REFERENCES narinfo_cache (store_path) ON DELETE CASCADE,
  project_id UUID NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
  build_id UUID REFERENCES builds (id) ON DELETE SET NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  PRIMARY KEY (store_path, project_id)
);

INSERT INTO
  narinfo_cache_projects (
    store_path,
    project_id,
    build_id,
    created_at,
    updated_at
  )
SELECT
  store_path,
  project_id,
  build_id,
  created_at,
  updated_at
FROM
  narinfo_cache
WHERE
  project_id IS NOT NULL
ON CONFLICT (store_path, project_id) DO UPDATE
SET
  build_id = COALESCE(
    EXCLUDED.build_id,
    narinfo_cache_projects.build_id
  ),
  updated_at = GREATEST(
    narinfo_cache_projects.updated_at,
    EXCLUDED.updated_at
  );

CREATE INDEX IF NOT EXISTS idx_narinfo_cache_projects_project_id ON narinfo_cache_projects (project_id);

CREATE INDEX IF NOT EXISTS idx_narinfo_cache_projects_build_id ON narinfo_cache_projects (build_id);
