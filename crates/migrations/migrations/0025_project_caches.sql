ALTER TABLE projects
ADD COLUMN cache_enabled BOOLEAN NOT NULL DEFAULT true,
ADD COLUMN cache_url TEXT,
ADD COLUMN cache_upstreams JSONB NOT NULL DEFAULT '[]'::jsonb;

ALTER TABLE narinfo_cache
ADD COLUMN build_id UUID REFERENCES builds (id) ON DELETE SET NULL,
ADD COLUMN project_id UUID REFERENCES projects (id) ON DELETE SET NULL;

UPDATE narinfo_cache n
SET
  build_id = src.build_id,
  project_id = src.project_id
FROM
  (
    SELECT DISTINCT
      ON (store_path) store_path,
      build_id,
      project_id
    FROM
      (
        SELECT
          bp.path AS store_path,
          b.id AS build_id,
          j.project_id
        FROM
          build_products bp
          JOIN builds b ON b.id = bp.build_id
          JOIN evaluations e ON e.id = b.evaluation_id
          JOIN jobsets j ON j.id = e.jobset_id
        UNION ALL
        SELECT
          b.build_output_path AS store_path,
          b.id AS build_id,
          j.project_id
        FROM
          builds b
          JOIN evaluations e ON e.id = b.evaluation_id
          JOIN jobsets j ON j.id = e.jobset_id
        WHERE
          b.build_output_path IS NOT NULL
      ) candidates
    ORDER BY
      store_path,
      build_id
  ) src
WHERE
  n.store_path = src.store_path;

CREATE INDEX idx_projects_cache_enabled ON projects (cache_enabled);

CREATE INDEX idx_narinfo_cache_project_id ON narinfo_cache (project_id);

CREATE INDEX idx_narinfo_cache_build_id ON narinfo_cache (build_id);
