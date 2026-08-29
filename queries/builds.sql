--: BuildRow(started_at?, completed_at?, log_path?, build_output_path?, error_message?, system?, notification_pending_since?, outputs?, constituents?, builder_id?, agent_machine_id?, fod_hash?, meta_description?, meta_license?, meta_homepage?, meta_maintainers?, started_notified_at?, effective_features?)

--! create (system?, outputs?, constituents?, fod_hash?, meta_description?, meta_license?, meta_homepage?, meta_maintainers?) : BuildRow
INSERT INTO builds (
  evaluation_id, job_name, drv_path, status, system, outputs, is_aggregate,
  constituents, is_fod, fod_hash, meta_description, meta_license,
  meta_homepage, meta_maintainers, required_features
)
VALUES (
  :evaluation_id, :job_name, :drv_path, 'pending', :system, :outputs,
  :is_aggregate, :constituents, :is_fod, :fod_hash, :meta_description,
  :meta_license, :meta_homepage, :meta_maintainers, :required_features
)
RETURNING *;

--! get_completed_by_drv_path : BuildRow
SELECT * FROM builds
WHERE drv_path = :drv_path AND status = 'succeeded'
LIMIT 1;

--! get : BuildRow
SELECT * FROM builds WHERE id = :id;

--! project_id_for_build
SELECT j.project_id
FROM builds b
JOIN evaluations e ON e.id = b.evaluation_id
JOIN jobsets j ON j.id = e.jobset_id
WHERE b.id = :id;

--! list_for_evaluation : BuildRow
SELECT * FROM builds
WHERE evaluation_id = :evaluation_id
ORDER BY created_at DESC;

--! list_for_jobset_evaluations : BuildRow
SELECT b.*
FROM builds b
JOIN evaluations e ON b.evaluation_id = e.id
WHERE e.jobset_id = :jobset_id AND b.evaluation_id = ANY(:evaluation_ids)
ORDER BY b.job_name ASC, e.evaluation_time DESC;

--! list_pending : BuildRow
WITH eligible_pending AS (
  SELECT b.*
  FROM builds b
  WHERE b.status = 'pending'
    AND NOT EXISTS (
      SELECT 1
      FROM build_dependencies bd
      JOIN builds dep ON dep.id = bd.dependency_build_id
      WHERE bd.build_id = b.id AND dep.status != 'succeeded'
    )
    AND NOT EXISTS (
      SELECT 1 FROM builds active
      WHERE active.drv_path = b.drv_path AND active.status = 'running'
    )
),
running_counts AS (
  SELECT e.jobset_id, COUNT(*) AS running
  FROM builds b
  JOIN evaluations e ON b.evaluation_id = e.id
  WHERE b.status = 'running'
  GROUP BY e.jobset_id
),
active_shares AS (
  SELECT
    j.id AS jobset_id,
    j.scheduling_shares,
    COALESCE(rc.running, 0) AS running,
    SUM(j.scheduling_shares) OVER () AS total_shares
  FROM jobsets j
  JOIN evaluations e2 ON e2.jobset_id = j.id
  JOIN eligible_pending b2 ON b2.evaluation_id = e2.id
  LEFT JOIN running_counts rc ON rc.jobset_id = j.id
  WHERE j.scheduling_shares > 0
  GROUP BY j.id, j.scheduling_shares, rc.running
)
SELECT b.*
FROM eligible_pending b
JOIN evaluations e ON b.evaluation_id = e.id
JOIN active_shares ash ON ash.jobset_id = e.jobset_id
ORDER BY
  b.priority DESC,
  cardinality(COALESCE(b.effective_features, b.required_features)) DESC,
  (ash.scheduling_shares::float / GREATEST(ash.total_shares, 1)
    - ash.running::float / GREATEST(:schedulable_capacity, 1)) DESC,
  b.created_at ASC,
  b.id ASC
LIMIT :limit;

--! start : BuildRow
WITH candidate AS (
  SELECT b.id
  FROM builds b
  WHERE b.id = :id AND b.status = 'pending'
    AND pg_try_advisory_xact_lock(hashtextextended(b.drv_path, 0))
    AND NOT EXISTS (
      SELECT 1 FROM builds active
      WHERE active.drv_path = b.drv_path AND active.status = 'running'
    )
  FOR UPDATE SKIP LOCKED
)
UPDATE builds
SET status = 'running', started_at = NOW()
FROM candidate
WHERE builds.id = candidate.id
RETURNING builds.*;

--! mark_started_notified
UPDATE builds
SET started_notified_at = NOW()
WHERE id = :id AND started_notified_at IS NULL
RETURNING id;

--! requeue : BuildRow
WITH bumped AS (
  UPDATE builds
  SET status = 'pending',
      started_at = NULL,
      completed_at = NULL,
      effective_features = NULL
  WHERE id = :id AND status = 'running'
  RETURNING *
)
SELECT * FROM bumped;

--! retry
UPDATE builds
SET status = 'pending',
    started_at = NULL,
    retry_count = retry_count + 1,
    completed_at = NULL,
    effective_features = NULL
WHERE id = :id;

--! complete (log_path?, build_output_path?, error_message?) : BuildRow
UPDATE builds
SET status = :status, completed_at = NOW(), log_path = :log_path,
    build_output_path = :build_output_path, error_message = :error_message
WHERE id = :id
RETURNING *;

--! list_pending_in_scheduler_order (system?, job_name?) : BuildRow
SELECT *
FROM builds
WHERE status = 'pending'
  AND (:system::text IS NULL OR system = :system)
  AND (:job_name::text IS NULL OR job_name ILIKE '%' || :job_name || '%')
ORDER BY
  priority DESC,
  cardinality(COALESCE(effective_features, required_features)) DESC,
  created_at ASC,
  id ASC
LIMIT :limit OFFSET :offset;

--! list_pending_for_systems : BuildRow
SELECT * FROM builds
WHERE status = 'pending' AND system = ANY(:systems)
ORDER BY priority DESC, created_at ASC
LIMIT 512;

--! pending_feature_demand
SELECT DISTINCT unnest(COALESCE(effective_features, required_features))
FROM builds
WHERE status = 'pending' AND system = :system;

--! bump_priority : BuildRow
UPDATE builds
SET priority = priority + :delta
WHERE id = :id AND status = 'pending'
RETURNING *;

--! list_recent : BuildRow
SELECT * FROM builds ORDER BY created_at DESC LIMIT :limit;

--! list_for_project : BuildRow
SELECT b.*
FROM builds b
JOIN evaluations e ON b.evaluation_id = e.id
JOIN jobsets j ON e.jobset_id = j.id
WHERE j.project_id = :project_id
ORDER BY b.created_at DESC;

--! get_stats : (total_builds?, completed_builds?, failed_builds?, running_builds?, pending_builds?, avg_duration_seconds?)
SELECT * FROM build_stats;

--! reset_orphaned
UPDATE builds
SET status = 'pending', started_at = NULL, effective_features = NULL
WHERE status = 'running'
  AND started_at < NOW() - make_interval(secs => :older_than_secs::bigint)
  AND NOT (id = ANY(:excluded_ids));

--! list_filtered (evaluation_id?, status?, system?, job_name?) : BuildRow
SELECT * FROM builds
WHERE (:evaluation_id::uuid IS NULL OR evaluation_id = :evaluation_id)
  AND (:status::text IS NULL OR status = :status)
  AND (:system::text IS NULL OR system = :system)
  AND (:job_name::text IS NULL OR job_name ILIKE '%' || :job_name || '%')
ORDER BY created_at DESC
LIMIT :limit OFFSET :offset;

--! count_filtered (evaluation_id?, status?, system?, job_name?)
SELECT COUNT(*) FROM builds
WHERE (:evaluation_id::uuid IS NULL OR evaluation_id = :evaluation_id)
  AND (:status::text IS NULL OR status = :status)
  AND (:system::text IS NULL OR system = :system)
  AND (:job_name::text IS NULL OR job_name ILIKE '%' || :job_name || '%');

--! get_cancelled_among
SELECT id FROM builds
WHERE id = ANY(:build_ids) AND status = 'cancelled';

--! cancel : BuildRow
UPDATE builds
SET status = 'cancelled', completed_at = NOW()
WHERE id = :id AND status IN ('pending', 'running')
RETURNING *;

--! cancel_cascade_dependents
SELECT build_id FROM build_dependencies WHERE dependency_build_id = :dependency_build_id;

--! restart : BuildRow
UPDATE builds
SET status = 'pending', started_at = NULL, completed_at = NULL,
    log_path = NULL, build_output_path = NULL, error_message = NULL,
    started_notified_at = NULL, effective_features = NULL,
    retry_count = retry_count + 1
WHERE id = :id
  AND status IN ('failed', 'succeeded', 'cancelled', 'cached_failure')
RETURNING *;

--! set_effective_features
UPDATE builds SET effective_features = :features WHERE id = :id;

--! mark_signed
UPDATE builds SET signed = true WHERE id = :id;

--! get_completed_by_drv_paths : BuildRow
SELECT DISTINCT ON (drv_path) *
FROM builds
WHERE drv_path = ANY(:drv_paths) AND status = 'succeeded'
ORDER BY drv_path, completed_at DESC;

--! list_pinned_ids
SELECT id FROM builds WHERE keep = true;

--! set_keep : BuildRow
UPDATE builds SET keep = :keep WHERE id = :id RETURNING *;

--! set_builder
UPDATE builds SET builder_id = :builder_id WHERE id = :id;

--! set_agent
UPDATE builds SET agent_machine_id = :machine_id WHERE id = :id;

--! list_constituents : BuildRow
SELECT b.*
FROM builds b
JOIN build_dependencies bd ON b.id = bd.dependency_build_id
WHERE bd.build_id = :build_id
ORDER BY b.created_at;

--! delete
DELETE FROM builds WHERE id = :id;
