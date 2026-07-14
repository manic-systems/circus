--: BuildDependencyRow()
--: BuildRow(started_at?, completed_at?, log_path?, build_output_path?, error_message?, system?, notification_pending_since?, outputs?, constituents?, builder_id?, agent_machine_id?, fod_hash?, meta_description?, meta_license?, meta_homepage?, meta_maintainers?, started_notified_at?, effective_features?)
--! create : BuildDependencyRow
INSERT INTO
  build_dependencies (build_id, dependency_build_id)
VALUES
  (:build_id,:dependency_build_id)
RETURNING
  *;

--! list_for_build : BuildDependencyRow
SELECT
  *
FROM
  build_dependencies
WHERE
  build_id =:build_id;

--! list_dependency_builds : BuildRow
SELECT
  b.*
FROM
  build_dependencies bd
  JOIN builds b ON b.id = bd.dependency_build_id
WHERE
  bd.build_id =:build_id
ORDER BY
  b.job_name;

--! list_dependent_builds : BuildRow
SELECT
  b.*
FROM
  build_dependencies bd
  JOIN builds b ON b.id = bd.build_id
WHERE
  bd.dependency_build_id =:dependency_build_id
ORDER BY
  b.job_name;

--! check_deps_for_builds
SELECT DISTINCT
  bd.build_id
FROM
  build_dependencies bd
  JOIN builds b ON bd.dependency_build_id = b.id
WHERE
  bd.build_id = ANY (:build_ids)
  AND b.status != 'succeeded';

--! all_deps_completed
SELECT
  COUNT(*)
FROM
  build_dependencies bd
  JOIN builds b ON bd.dependency_build_id = b.id
WHERE
  bd.build_id =:build_id
  AND b.status != 'succeeded';
