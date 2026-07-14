--: BuildStepRow(output?, error_output?, completed_at?, exit_code?)
--! create : BuildStepRow
INSERT INTO
  build_steps (build_id, step_number, command)
VALUES
  (:build_id,:step_number,:command)
RETURNING
  *;

--! complete (output?, error_output?) : BuildStepRow
UPDATE build_steps
SET
  completed_at = NOW(),
  exit_code =:exit_code,
  output =:output,
  error_output =:error_output
WHERE
  id =:id
RETURNING
  *;

--! list_for_build : BuildStepRow
SELECT
  *
FROM
  build_steps
WHERE
  build_id =:build_id
ORDER BY
  step_number ASC;

--! delete_for_build
DELETE FROM build_steps
WHERE
  build_id =:build_id;
