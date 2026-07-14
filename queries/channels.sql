--: ChannelRow(current_evaluation_id?)
--! create : ChannelRow
INSERT INTO
  channels (project_id, name, jobset_id)
VALUES
  (:project_id,:name,:jobset_id)
RETURNING
  *;

--! get : ChannelRow
SELECT
  *
FROM
  channels
WHERE
  id =:id;

--! list_for_project : ChannelRow
SELECT
  *
FROM
  channels
WHERE
  project_id =:project_id
ORDER BY
  name;

--! list_all : ChannelRow
SELECT
  *
FROM
  channels
ORDER BY
  name;

--! count
SELECT
  COUNT(*)
FROM
  channels;

--! get_by_name : ChannelRow
SELECT
  *
FROM
  channels
WHERE
  name =:name
ORDER BY
  created_at DESC,
  id DESC
LIMIT
  1;

--! promote : ChannelRow
UPDATE channels
SET
  current_evaluation_id =:evaluation_id,
  updated_at = NOW()
WHERE
  id =:channel_id
RETURNING
  *;

--! delete
DELETE FROM channels
WHERE
  id =:id;

--! upsert : ChannelRow
INSERT INTO
  channels (project_id, name, jobset_id)
VALUES
  (:project_id,:name,:jobset_id)
ON CONFLICT (project_id, name) DO UPDATE
SET
  jobset_id = EXCLUDED.jobset_id
RETURNING
  *;

--! sync_for_project_delete
DELETE FROM channels
WHERE
  project_id =:project_id
  AND name != ALL (:names::text[]);

--! auto_promote_count : (total?, completed?)
SELECT
  COUNT(*) AS total,
  COUNT(*) FILTER (
    WHERE
      status = 'succeeded'
  ) AS completed
FROM
  builds
WHERE
  evaluation_id =:evaluation_id;

--! auto_promote_channels : ChannelRow
SELECT
  *
FROM
  channels
WHERE
  jobset_id =:jobset_id;
