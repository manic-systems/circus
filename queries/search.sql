--: ProjectQuickSearchRow(description?, cache_url?)
--: BuildQuickSearchRow(started_at?, completed_at?, log_path?, build_output_path?, error_message?, system?, notification_pending_since?, outputs?, constituents?, builder_id?, agent_machine_id?, fod_hash?, meta_description?, meta_license?, meta_homepage?, meta_maintainers?, started_notified_at?, effective_features?)

--! quick_projects : ProjectQuickSearchRow
SELECT
  *
FROM
  projects
WHERE
  name ILIKE :pattern
  OR description ILIKE :pattern
ORDER BY
  name
LIMIT
  :limit;

--! quick_builds : BuildQuickSearchRow
SELECT
  *
FROM
  builds
WHERE
  job_name ILIKE :pattern
  OR drv_path ILIKE :pattern
ORDER BY
  created_at DESC
LIMIT
  :limit;

--: JobsetSearchRow(branch?, branch_pattern?, tag_pattern?, last_checked_at?)
--: EvaluationSearchRow(error_message?, inputs_hash?, pr_number?, pr_head_branch?, pr_base_branch?, pr_action?, started_at?)

-- The optional filters use NULL-guarded predicates and the dynamic sort is a
-- CASE ladder keyed on :sort, so the whole search surface stays static.

--! search_projects (created_after?, created_before?, has_jobsets?, sort?) : ProjectQuickSearchRow
SELECT p.*
FROM projects p
WHERE (p.name ILIKE :pattern OR p.description ILIKE :pattern)
  AND (:created_after::timestamptz IS NULL OR p.created_at >= :created_after)
  AND (:created_before::timestamptz IS NULL OR p.created_at <= :created_before)
  AND (:has_jobsets::bool IS NULL
    OR :has_jobsets = EXISTS (SELECT 1 FROM jobsets j WHERE j.project_id = p.id))
ORDER BY
  CASE WHEN :sort = 'name_asc' THEN p.name END ASC,
  CASE WHEN :sort = 'name_desc' THEN p.name END DESC,
  CASE WHEN :sort = 'created_at_asc' THEN p.created_at END ASC,
  CASE WHEN :sort = 'created_at_desc' THEN p.created_at END DESC,
  p.name ASC
LIMIT :limit OFFSET :offset;

--! count_projects (created_after?, created_before?, has_jobsets?)
SELECT COUNT(*)
FROM projects p
WHERE (p.name ILIKE :pattern OR p.description ILIKE :pattern)
  AND (:created_after::timestamptz IS NULL OR p.created_at >= :created_after)
  AND (:created_before::timestamptz IS NULL OR p.created_at <= :created_before)
  AND (:has_jobsets::bool IS NULL
    OR :has_jobsets = EXISTS (SELECT 1 FROM jobsets j WHERE j.project_id = p.id));

--! search_jobsets (project_id?, enabled?, flake_mode?) : JobsetSearchRow
SELECT *
FROM jobsets
WHERE name ILIKE :pattern
  AND (:project_id::uuid IS NULL OR project_id = :project_id)
  AND (:enabled::bool IS NULL OR enabled = :enabled)
  AND (:flake_mode::bool IS NULL OR flake_mode = :flake_mode)
ORDER BY name ASC
LIMIT :limit OFFSET :offset;

--! count_jobsets (project_id?, enabled?, flake_mode?)
SELECT COUNT(*)
FROM jobsets
WHERE name ILIKE :pattern
  AND (:project_id::uuid IS NULL OR project_id = :project_id)
  AND (:enabled::bool IS NULL OR enabled = :enabled)
  AND (:flake_mode::bool IS NULL OR flake_mode = :flake_mode);

--! search_evaluations (project_id?, jobset_id?, has_builds?, finished_after?, finished_before?) : EvaluationSearchRow
SELECT e.*
FROM evaluations e
JOIN jobsets j ON j.id = e.jobset_id
WHERE (:project_id::uuid IS NULL OR j.project_id = :project_id)
  AND (:jobset_id::uuid IS NULL OR e.jobset_id = :jobset_id)
  AND (:has_builds::bool IS NULL
    OR :has_builds = EXISTS (SELECT 1 FROM builds b WHERE b.evaluation_id = e.id))
  AND (:finished_after::timestamptz IS NULL
    OR e.evaluation_time >= :finished_after)
  AND (:finished_before::timestamptz IS NULL
    OR e.evaluation_time <= :finished_before)
ORDER BY e.evaluation_time DESC
LIMIT :limit OFFSET :offset;

--! count_evaluations (project_id?, jobset_id?, has_builds?, finished_after?, finished_before?)
SELECT COUNT(*)
FROM evaluations e
JOIN jobsets j ON j.id = e.jobset_id
WHERE (:project_id::uuid IS NULL OR j.project_id = :project_id)
  AND (:jobset_id::uuid IS NULL OR e.jobset_id = :jobset_id)
  AND (:has_builds::bool IS NULL
    OR :has_builds = EXISTS (SELECT 1 FROM builds b WHERE b.evaluation_id = e.id))
  AND (:finished_after::timestamptz IS NULL
    OR e.evaluation_time >= :finished_after)
  AND (:finished_before::timestamptz IS NULL
    OR e.evaluation_time <= :finished_before);

--! search_builds (status?, project_id?, jobset_id?, evaluation_id?, created_after?, created_before?, min_priority?, max_priority?, sort?) : BuildQuickSearchRow
SELECT b.*
FROM builds b
JOIN evaluations e ON e.id = b.evaluation_id
JOIN jobsets j ON j.id = e.jobset_id
WHERE (b.job_name ILIKE :pattern OR b.drv_path ILIKE :pattern)
  AND (:status::text IS NULL OR b.status = :status)
  AND (:project_id::uuid IS NULL OR j.project_id = :project_id)
  AND (:jobset_id::uuid IS NULL OR e.jobset_id = :jobset_id)
  AND (:evaluation_id::uuid IS NULL OR b.evaluation_id = :evaluation_id)
  AND (:created_after::timestamptz IS NULL OR b.created_at >= :created_after)
  AND (:created_before::timestamptz IS NULL OR b.created_at <= :created_before)
  AND (:min_priority::int IS NULL OR b.priority >= :min_priority)
  AND (:max_priority::int IS NULL OR b.priority <= :max_priority)
ORDER BY
  CASE WHEN :sort = 'created_at_asc' THEN b.created_at END ASC,
  CASE WHEN :sort = 'created_at_desc' THEN b.created_at END DESC,
  CASE WHEN :sort = 'job_name_asc' THEN b.job_name END ASC,
  CASE WHEN :sort = 'job_name_desc' THEN b.job_name END DESC,
  CASE WHEN :sort = 'status_asc' THEN b.status END ASC,
  CASE WHEN :sort = 'status_desc' THEN b.status END DESC,
  CASE WHEN :sort = 'priority_asc' THEN b.priority END ASC,
  CASE WHEN :sort = 'priority_desc' THEN b.priority END DESC,
  b.created_at DESC
LIMIT :limit OFFSET :offset;

--! count_builds (status?, project_id?, jobset_id?, evaluation_id?, created_after?, created_before?, min_priority?, max_priority?)
SELECT COUNT(*)
FROM builds b
JOIN evaluations e ON e.id = b.evaluation_id
JOIN jobsets j ON j.id = e.jobset_id
WHERE (b.job_name ILIKE :pattern OR b.drv_path ILIKE :pattern)
  AND (:status::text IS NULL OR b.status = :status)
  AND (:project_id::uuid IS NULL OR j.project_id = :project_id)
  AND (:jobset_id::uuid IS NULL OR e.jobset_id = :jobset_id)
  AND (:evaluation_id::uuid IS NULL OR b.evaluation_id = :evaluation_id)
  AND (:created_after::timestamptz IS NULL OR b.created_at >= :created_after)
  AND (:created_before::timestamptz IS NULL OR b.created_at <= :created_before)
  AND (:min_priority::int IS NULL OR b.priority >= :min_priority)
  AND (:max_priority::int IS NULL OR b.priority <= :max_priority);
