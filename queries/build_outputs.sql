--: BuildOutputRow(path?)
--! create (path?) : BuildOutputRow
INSERT INTO
  build_outputs (build, name, path)
VALUES
  (:build,:name,:path)
RETURNING
  *;

--! list_for_build : BuildOutputRow
SELECT
  *
FROM
  build_outputs
WHERE
  build =:build
ORDER BY
  name ASC;

--! find_by_path : BuildOutputRow
SELECT
  *
FROM
  build_outputs
WHERE
  path =:path
ORDER BY
  build,
  name;

--! delete_for_build
DELETE FROM build_outputs
WHERE
  build =:build;
