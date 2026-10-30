--! upsert
INSERT INTO effect_task_tokens (build_id, retry_count, token_hash)
VALUES (:build_id, :retry_count, :token_hash)
ON CONFLICT (build_id) DO UPDATE
SET retry_count = EXCLUDED.retry_count,
    token_hash = EXCLUDED.token_hash,
    created_at = NOW();

--! active_project : (project_id, build_id)
SELECT j.project_id, b.id AS build_id
FROM effect_task_tokens t
JOIN builds b ON b.id = t.build_id
JOIN evaluations e ON e.id = b.evaluation_id
JOIN jobsets j ON j.id = e.jobset_id
WHERE t.token_hash = :token_hash
  AND b.retry_count = t.retry_count
  AND b.kind = 'effect'
  AND b.status = 'running'
  AND b.effect_execution_active;
