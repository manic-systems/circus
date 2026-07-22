--: EffectCompletionEventRow(acknowledged_at?)

--! list_pending : EffectCompletionEventRow
SELECT
  build_id,
  retry_count,
  revision,
  build_snapshot,
  created_at,
  updated_at,
  acknowledged_at
FROM effect_completion_events
WHERE acknowledged_at IS NULL
ORDER BY created_at, build_id, retry_count
LIMIT :limit;

--! ack
UPDATE effect_completion_events
SET acknowledged_at = NOW()
WHERE build_id = :build_id
  AND retry_count = :retry_count
  AND revision = :revision
  AND acknowledged_at IS NULL
RETURNING build_id;
