--: ProjectRow(description?, cache_url?)

--! create (description?, cache_url?) : ProjectRow
INSERT INTO projects (name, description, repository_url, cache_enabled, cache_url, cache_upstreams)
VALUES (:name, :description, :repository_url, :cache_enabled, :cache_url, :cache_upstreams)
RETURNING *;

--! get : ProjectRow
SELECT * FROM projects WHERE id = :id;

--! get_by_name : ProjectRow
SELECT * FROM projects WHERE name = :name;

--! list : ProjectRow
SELECT * FROM projects ORDER BY created_at DESC LIMIT :limit OFFSET :offset;

--! count
SELECT COUNT(*) FROM projects;

--! update (description?, cache_url?) : ProjectRow
UPDATE projects
SET name = :name, description = :description, repository_url = :repository_url,
    cache_enabled = :cache_enabled, cache_url = :cache_url, cache_upstreams = :cache_upstreams
WHERE id = :id
RETURNING *;

--! upsert (description?, cache_url?) : ProjectRow
INSERT INTO projects (name, description, repository_url, cache_enabled, cache_url, cache_upstreams)
VALUES (:name, :description, :repository_url, :cache_enabled, :cache_url, :cache_upstreams)
ON CONFLICT (name) DO UPDATE
SET description = EXCLUDED.description, repository_url = EXCLUDED.repository_url,
    cache_enabled = EXCLUDED.cache_enabled, cache_url = EXCLUDED.cache_url,
    cache_upstreams = EXCLUDED.cache_upstreams
RETURNING *;

--! list_without_active_jobsets : ProjectRow
SELECT p.*
FROM projects p
WHERE NOT EXISTS (SELECT 1 FROM jobsets j WHERE j.project_id = p.id);

--! delete
DELETE FROM projects WHERE id = :id;
