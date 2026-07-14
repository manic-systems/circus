--: ProjectMemberRow()
--! create : ProjectMemberRow
INSERT INTO
  project_members (project_id, user_id, role)
VALUES
  (:project_id,:user_id,:role)
RETURNING
  *;

--! get : ProjectMemberRow
SELECT
  *
FROM
  project_members
WHERE
  id =:id;

--! get_by_project_and_user : ProjectMemberRow
SELECT
  *
FROM
  project_members
WHERE
  project_id =:project_id
  AND user_id =:user_id;

--! list_for_project : ProjectMemberRow
SELECT
  *
FROM
  project_members
WHERE
  project_id =:project_id
ORDER BY
  created_at;

--! list_for_user : ProjectMemberRow
SELECT
  *
FROM
  project_members
WHERE
  user_id =:user_id
ORDER BY
  created_at;

--! update : ProjectMemberRow
UPDATE project_members
SET role =:role
WHERE
  id =:id
RETURNING
  *;

--! delete
DELETE FROM project_members
WHERE
  id =:id;

--! delete_by_project_and_user
DELETE FROM project_members
WHERE
  project_id =:project_id
  AND user_id =:user_id;

--! upsert : ProjectMemberRow
INSERT INTO
  project_members (project_id, user_id, role)
VALUES
  (:project_id,:user_id,:role)
ON CONFLICT (project_id, user_id) DO UPDATE
SET role = EXCLUDED.role
RETURNING
  *;

--! sync_delete_removed
DELETE FROM project_members
WHERE
  project_id =:project_id
  AND user_id != ALL (:user_ids::uuid[]);
