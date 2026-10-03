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
