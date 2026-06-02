-- Count pending and total builds across the whole builds table. Pending
-- rows have NULL started_at until the queue runner claims them, so any
-- WHERE started_at IS NOT NULL filter on this view would silently zero
-- out the pending and total counts. AVG() ignores NULL inputs, so
-- avg_duration_seconds remains correct without filtering.
CREATE OR REPLACE VIEW build_stats AS
SELECT
  COUNT(*) AS total_builds,
  COUNT(
    CASE
      WHEN status = 'succeeded' THEN 1
    END
  ) AS completed_builds,
  COUNT(
    CASE
      WHEN status = 'failed' THEN 1
    END
  ) AS failed_builds,
  COUNT(
    CASE
      WHEN status = 'running' THEN 1
    END
  ) AS running_builds,
  COUNT(
    CASE
      WHEN status = 'pending' THEN 1
    END
  ) AS pending_builds,
  AVG(
    EXTRACT(
      EPOCH
      FROM
        (completed_at - started_at)
    )
  )::double precision AS avg_duration_seconds
FROM
  builds;
