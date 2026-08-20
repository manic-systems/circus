--: EvaluationRow(error_message?, inputs_hash?, pr_number?, pr_head_branch?, pr_base_branch?, pr_action?, started_at?, source_scope?, superseded_by?)

--! create_with_kind (pr_number?, pr_head_branch?, pr_base_branch?, pr_action?, source_scope?) : EvaluationRow
INSERT INTO evaluations (
  jobset_id, commit_hash, status, trigger_kind,
  pr_number, pr_head_branch, pr_base_branch, pr_action, started_at,
  source_scope
)
VALUES (
  :jobset_id, :commit_hash, :status, :trigger_kind,
  :pr_number, :pr_head_branch, :pr_base_branch, :pr_action,
  CASE WHEN :status::text = 'running' THEN NOW() END, :source_scope
)
RETURNING *;

--! get : EvaluationRow
SELECT * FROM evaluations WHERE id = :id;

--! get_visible : EvaluationRow
SELECT * FROM evaluations
WHERE id = :id AND (:include_hidden::boolean OR hidden = false);

--! list_for_jobset : EvaluationRow
SELECT * FROM evaluations WHERE jobset_id = :jobset_id ORDER BY evaluation_time DESC;

--! list_filtered_with_visibility (jobset_id?, status?) : EvaluationRow
SELECT * FROM evaluations
WHERE (:jobset_id::uuid IS NULL OR jobset_id = :jobset_id)
  AND (:status::text IS NULL OR status = :status)
  AND (:include_hidden::boolean OR hidden = false)
ORDER BY evaluation_time DESC
LIMIT :limit OFFSET :offset;

--! count_filtered_with_visibility (jobset_id?, status?)
SELECT COUNT(*) FROM evaluations
WHERE (:jobset_id::uuid IS NULL OR jobset_id = :jobset_id)
  AND (:status::text IS NULL OR status = :status)
  AND (:include_hidden::boolean OR hidden = false);

--! set_hidden : EvaluationRow
UPDATE evaluations SET hidden = :hidden WHERE id = :id RETURNING *;

--! try_claim_pending : EvaluationRow
UPDATE evaluations SET status = 'running', started_at = NOW()
WHERE id = :id AND status = 'pending'
RETURNING *;

--! update_status (error_message?) : EvaluationRow
UPDATE evaluations SET status = :status, error_message = :error_message
WHERE id = :id
RETURNING *;

--! get_latest : EvaluationRow
SELECT * FROM evaluations
WHERE jobset_id = :jobset_id AND status = 'completed'
ORDER BY evaluation_time DESC
LIMIT 1;

--! set_inputs_hash
UPDATE evaluations SET inputs_hash = :inputs_hash WHERE id = :id;

--! get_by_inputs_hash : EvaluationRow
SELECT * FROM evaluations
WHERE jobset_id = :jobset_id AND inputs_hash = :inputs_hash AND status = 'completed'
ORDER BY evaluation_time DESC
LIMIT 1;

--! count
SELECT COUNT(*) FROM evaluations;

--! list_pending : EvaluationRow
SELECT * FROM evaluations WHERE status = 'pending' ORDER BY evaluation_time ASC;

--! list_jobsets_with_pending
SELECT DISTINCT jobset_id FROM evaluations WHERE status = 'pending';

--! get_source_head
SELECT commit_hash FROM evaluation_source_heads
WHERE jobset_id = :jobset_id AND source_scope = :source_scope;

--! set_source_head
INSERT INTO evaluation_source_heads (jobset_id, source_scope, commit_hash)
VALUES (:jobset_id, :source_scope, :commit_hash)
ON CONFLICT (jobset_id, source_scope)
DO UPDATE SET commit_hash = EXCLUDED.commit_hash;

--! get_by_jobset_and_commit : EvaluationRow
SELECT * FROM evaluations
WHERE jobset_id = :jobset_id AND commit_hash = :commit_hash
ORDER BY (trigger_kind = 'interval') ASC, evaluation_time DESC
LIMIT 1;

--: BuildContextRow()

--! get_build_contexts : BuildContextRow
SELECT
  e.id AS evaluation_id,
  p.id AS project_id,
  p.name AS project_name,
  j.id AS jobset_id,
  j.name AS jobset_name
FROM evaluations e
JOIN jobsets j ON e.jobset_id = j.id
JOIN projects p ON j.project_id = p.id
WHERE e.id = ANY(:evaluation_ids);

--! finish_running (error_message?) : EvaluationRow
UPDATE evaluations SET status = :status, error_message = :error_message
WHERE id = :id AND status = 'running'
RETURNING *;

--! cancel : EvaluationRow
UPDATE evaluations SET status = 'cancelled', error_message = NULL
WHERE id = :id AND status IN ('pending', 'running')
RETURNING *;

--! sweep_orphaned : EvaluationRow
UPDATE evaluations
SET status = CASE WHEN orphaned_count >= 2 THEN 'failed' ELSE 'pending' END,
    error_message = CASE WHEN orphaned_count >= 2
      THEN 'evaluation orphaned repeatedly, giving up' END,
    orphaned_count = orphaned_count + 1,
    started_at = NULL, inputs_hash = NULL, evaluation_time = NOW()
WHERE status = 'running'
  AND COALESCE(started_at, evaluation_time)
    < NOW() - make_interval(secs => :deadline_secs)
RETURNING *;

--! supersede_source_evaluations (source_scope?)
UPDATE evaluations
SET status = CASE WHEN status IN ('pending', 'running')
      THEN 'cancelled' ELSE status END,
    error_message = CASE WHEN status IN ('pending', 'running')
      THEN 'superseded by evaluation ' || :superseded_by::text
      ELSE error_message END,
    superseded_by = :superseded_by
WHERE jobset_id = :jobset_id AND id <> :superseded_by
  AND trigger_kind = 'source_change'
  AND source_scope = :source_scope
  AND (status IN ('pending', 'running') OR EXISTS (
    SELECT 1 FROM builds b
    WHERE b.evaluation_id = evaluations.id
      AND b.status IN ('pending', 'running')
  ));

--! cancel_superseded_builds (source_scope?)
UPDATE builds b
SET status = 'cancelled', completed_at = NOW(),
    error_message = 'superseded by evaluation ' || :superseded_by::text
FROM evaluations e
WHERE b.evaluation_id = e.id
  AND e.jobset_id = :jobset_id AND e.id <> :superseded_by
  AND e.trigger_kind = 'source_change'
  AND e.source_scope = :source_scope
  AND b.status IN ('pending', 'running');

--! restart_requeue : EvaluationRow
UPDATE evaluations e
SET status = 'pending', evaluation_time = NOW(),
    error_message = NULL, inputs_hash = NULL,
    started_at = NULL, orphaned_count = 0, superseded_by = NULL,
    trigger_kind = CASE WHEN e.trigger_kind = 'source_change'
      THEN 'manual' ELSE e.trigger_kind END,
    source_scope = NULL
FROM jobsets j
WHERE e.id = :id AND e.jobset_id = j.id
  AND e.status IN ('cancelled', 'failed', 'timed_out')
  AND (j.state = 'one_shot'
    OR (j.enabled AND j.state IN ('enabled', 'one_at_a_time')))
RETURNING e.*;

--! restart_delete_builds
DELETE FROM builds WHERE evaluation_id = :id;

--! restart_reenable_one_shot
UPDATE jobsets SET enabled = true
WHERE id = (SELECT jobset_id FROM evaluations WHERE id = :id)
  AND state = 'one_shot';

--! lock_running
SELECT id FROM evaluations WHERE id = :id AND status = 'running' FOR UPDATE;

--! status_of
SELECT status FROM evaluations WHERE id = :id;
--! list_page_filtered (project?, jobset?, commit?, status?) : EvaluationRow
SELECT e.* FROM evaluations e
JOIN jobsets j ON j.id = e.jobset_id
JOIN projects p ON p.id = j.project_id
WHERE (:project::text IS NULL OR p.name ILIKE '%' || :project || '%')
  AND (:jobset::text IS NULL OR j.name ILIKE '%' || :jobset || '%')
  AND (:commit::text IS NULL OR e.commit_hash ILIKE :commit || '%')
  AND (:status::text IS NULL OR e.status = :status)
  AND (:include_hidden OR e.hidden = false)
ORDER BY e.evaluation_time DESC
LIMIT :limit OFFSET :offset;

--! count_page_filtered (project?, jobset?, commit?, status?)
SELECT COUNT(*) FROM evaluations e
JOIN jobsets j ON j.id = e.jobset_id
JOIN projects p ON p.id = j.project_id
WHERE (:project::text IS NULL OR p.name ILIKE '%' || :project || '%')
  AND (:jobset::text IS NULL OR j.name ILIKE '%' || :jobset || '%')
  AND (:commit::text IS NULL OR e.commit_hash ILIKE :commit || '%')
  AND (:status::text IS NULL OR e.status = :status)
  AND (:include_hidden OR e.hidden = false);
