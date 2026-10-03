--! upsert
INSERT INTO build_closure_diffs (build_id, against_build_id, changes)
VALUES (:build_id, :against_build_id, :changes)
ON CONFLICT (build_id) DO UPDATE
SET against_build_id = EXCLUDED.against_build_id, changes = EXCLUDED.changes;

--! get : (against_build_id, against_commit, changes)
SELECT d.against_build_id, e.commit_hash AS against_commit, d.changes
FROM build_closure_diffs d
JOIN builds b ON b.id = d.against_build_id
JOIN evaluations e ON e.id = b.evaluation_id
WHERE d.build_id = :build_id;

--! latest_measured_success
WITH current_build AS (
  SELECT
    b.job_name, b.system, e.jobset_id, e.evaluation_time,
    CASE
      WHEN e.pr_number IS NULL THEN e.source_scope
      ELSE 'branch:' || e.pr_base_branch
    END AS scope,
    j.branch_pattern IS NOT NULL OR j.tag_pattern IS NOT NULL AS multi_ref
  FROM builds b
  JOIN evaluations e ON e.id = b.evaluation_id
  JOIN jobsets j ON j.id = e.jobset_id
  WHERE b.id = :build_id
)
SELECT b.id
FROM current_build c
JOIN evaluations e ON e.jobset_id = c.jobset_id
  AND e.pr_number IS NULL
  AND (NOT c.multi_ref
    OR e.source_scope = c.scope
    OR e.source_scope IS NULL
    OR c.scope IS NULL)
  AND e.evaluation_time < c.evaluation_time
JOIN builds b ON b.evaluation_id = e.id
  AND b.job_name = c.job_name
  AND b.system IS NOT DISTINCT FROM c.system
  AND b.status = 'succeeded'
  AND b.closure_size IS NOT NULL
  AND EXISTS (
    SELECT 1 FROM build_outputs o WHERE o.build = b.id AND o.path IS NOT NULL
  )
ORDER BY e.evaluation_time DESC
LIMIT 1;
