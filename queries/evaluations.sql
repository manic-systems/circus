--: EvaluationRow(error_message?, inputs_hash?, pr_number?, pr_head_branch?, pr_base_branch?, pr_action?)

--! create_with_kind (pr_number?, pr_head_branch?, pr_base_branch?, pr_action?) : EvaluationRow
INSERT INTO evaluations (
  jobset_id, commit_hash, status, trigger_kind,
  pr_number, pr_head_branch, pr_base_branch, pr_action
)
VALUES (
  :jobset_id, :commit_hash, :status, :trigger_kind,
  :pr_number, :pr_head_branch, :pr_base_branch, :pr_action
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
UPDATE evaluations SET status = 'running'
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

--! restart_requeue : EvaluationRow
UPDATE evaluations e
SET status = 'pending', evaluation_time = NOW(),
    error_message = NULL, inputs_hash = NULL
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
