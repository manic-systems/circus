--: JobsetInputRow(revision?)
--! create (revision?) : JobsetInputRow
INSERT INTO
  jobset_inputs (jobset_id, name, input_type, value, revision)
VALUES
  (:jobset_id,:name,:input_type,:value,:revision)
RETURNING
  *;

--! list_for_jobset : JobsetInputRow
SELECT
  *
FROM
  jobset_inputs
WHERE
  jobset_id =:jobset_id
ORDER BY
  name ASC;

--! delete
DELETE FROM jobset_inputs
WHERE
  id =:id;

--! upsert (revision?) : JobsetInputRow
INSERT INTO
  jobset_inputs (jobset_id, name, input_type, value, revision)
VALUES
  (:jobset_id,:name,:input_type,:value,:revision)
ON CONFLICT (jobset_id, name) DO UPDATE
SET
  input_type = EXCLUDED.input_type,
  value = EXCLUDED.value,
  revision = EXCLUDED.revision
RETURNING
  *;

--! sync_for_jobset_delete
DELETE FROM jobset_inputs
WHERE
  jobset_id =:jobset_id
  AND name != ALL (:names::text[]);
