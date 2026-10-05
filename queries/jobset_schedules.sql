--: JobsetScheduleRow(last_fired_at?)

--! list_for_jobset : JobsetScheduleRow
SELECT jobset_id, name, when_spec, commit_hash, next_due_at, last_fired_at
FROM jobset_schedules
WHERE jobset_id = :jobset_id;

--! upsert
INSERT INTO jobset_schedules (jobset_id, name, when_spec, commit_hash, next_due_at)
VALUES (:jobset_id, :name, :when_spec, :commit_hash, :next_due_at)
ON CONFLICT (jobset_id, name) DO UPDATE
SET when_spec = EXCLUDED.when_spec,
    commit_hash = EXCLUDED.commit_hash,
    next_due_at = EXCLUDED.next_due_at;

--! delete_except
DELETE FROM jobset_schedules
WHERE jobset_id = :jobset_id AND NOT (name = ANY(:names));

--! list_due : JobsetScheduleRow
SELECT jobset_id, name, when_spec, commit_hash, next_due_at, last_fired_at
FROM jobset_schedules
WHERE next_due_at <= NOW()
ORDER BY next_due_at;

--! mark_fired
UPDATE jobset_schedules
SET last_fired_at = NOW(), next_due_at = :next_due_at
WHERE jobset_id = :jobset_id AND name = :name AND next_due_at = :previous_due_at
RETURNING name;
