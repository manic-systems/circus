--! get
SELECT data
FROM project_state_files
WHERE project_id = :project_id AND name = :name;

--! put
INSERT INTO project_state_files (project_id, name, data, updated_by_build)
VALUES (:project_id, :name, :data, :build_id)
ON CONFLICT (project_id, name) DO UPDATE
SET data = EXCLUDED.data,
    updated_at = NOW(),
    updated_by_build = EXCLUDED.updated_by_build;
