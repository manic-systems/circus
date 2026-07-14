--: BuildMetricRow()
--! upsert : BuildMetricRow
INSERT INTO
  build_metrics (build_id, metric_name, metric_value, unit)
VALUES
  (:build_id,:metric_name,:metric_value,:unit)
ON CONFLICT (build_id, metric_name) DO UPDATE
SET
  metric_value = EXCLUDED.metric_value,
  collected_at = NOW()
RETURNING
  *;

--! calculate_failure_rate (project_id?, jobset_id?) : (id, status)
SELECT
  b.id,
  b.status::text AS status
FROM
  builds b
  JOIN evaluations e ON b.evaluation_id = e.id
  JOIN jobsets j ON e.jobset_id = j.id
WHERE
  (
:project_id::uuid IS NULL
    OR j.project_id =:project_id
  )
  AND (
:jobset_id::uuid IS NULL
    OR j.id =:jobset_id
  )
  AND b.completed_at > NOW() - (INTERVAL '1 minute' *:window_minutes)
ORDER BY
  b.completed_at DESC;

--! get_build_stats_timeseries (project_id?, jobset_id?) : (avg_duration?)
SELECT
  date_trunc('minute', b.completed_at) + (
    EXTRACT(
      MINUTE
      FROM
        b.completed_at
    )::int /:bucket_minutes
  ) * INTERVAL '1 minute' *:bucket_minutes AS bucket_time,
  COUNT(*) AS total_builds,
  COUNT(*) FILTER (
    WHERE
      b.status = 'failed'
  ) AS failed_builds,
  AVG(
    EXTRACT(
      EPOCH
      FROM
        (b.completed_at - b.started_at)
    )
  ) AS avg_duration
FROM
  builds b
  JOIN evaluations e ON b.evaluation_id = e.id
  JOIN jobsets j ON e.jobset_id = j.id
WHERE
  b.completed_at IS NOT NULL
  AND b.completed_at > NOW() - (INTERVAL '1 hour' *:hours)
  AND (
:project_id::uuid IS NULL
    OR j.project_id =:project_id
  )
  AND (
:jobset_id::uuid IS NULL
    OR j.id =:jobset_id
  )
GROUP BY
  bucket_time
ORDER BY
  bucket_time ASC;

--! get_duration_percentiles_timeseries (project_id?, jobset_id?) : (p50?, p95?, p99?)
SELECT
  date_trunc('minute', b.completed_at) + (
    EXTRACT(
      MINUTE
      FROM
        b.completed_at
    )::int /:bucket_minutes
  ) * INTERVAL '1 minute' *:bucket_minutes AS bucket_time,
  PERCENTILE_CONT(0.5) WITHIN GROUP (
    ORDER BY
      EXTRACT(
        EPOCH
        FROM
          (b.completed_at - b.started_at)
      )
  ) AS p50,
  PERCENTILE_CONT(0.95) WITHIN GROUP (
    ORDER BY
      EXTRACT(
        EPOCH
        FROM
          (b.completed_at - b.started_at)
      )
  ) AS p95,
  PERCENTILE_CONT(0.99) WITHIN GROUP (
    ORDER BY
      EXTRACT(
        EPOCH
        FROM
          (b.completed_at - b.started_at)
      )
  ) AS p99
FROM
  builds b
  JOIN evaluations e ON b.evaluation_id = e.id
  JOIN jobsets j ON e.jobset_id = j.id
WHERE
  b.completed_at IS NOT NULL
  AND b.started_at IS NOT NULL
  AND b.completed_at > NOW() - (INTERVAL '1 hour' *:hours)
  AND (
:project_id::uuid IS NULL
    OR j.project_id =:project_id
  )
  AND (
:jobset_id::uuid IS NULL
    OR j.id =:jobset_id
  )
GROUP BY
  bucket_time
ORDER BY
  bucket_time ASC;

--! get_queue_depth_timeseries : (bucket_time, pending_count)
SELECT
  date_trunc('minute', created_at) + (
    EXTRACT(
      MINUTE
      FROM
        created_at
    )::int /:bucket_minutes
  ) * INTERVAL '1 minute' *:bucket_minutes AS bucket_time,
  COUNT(*) FILTER (
    WHERE
      status = 'pending'
  ) AS pending_count
FROM
  builds
WHERE
  created_at > NOW() - (INTERVAL '1 hour' *:hours)
GROUP BY
  bucket_time
ORDER BY
  bucket_time ASC;

--! get_system_distribution (project_id?) : (system, build_count)
SELECT
  COALESCE(b.system, 'unknown') AS system,
  COUNT(*) AS build_count
FROM
  builds b
  JOIN evaluations e ON b.evaluation_id = e.id
  JOIN jobsets j ON e.jobset_id = j.id
WHERE
  b.completed_at > NOW() - (INTERVAL '1 hour' *:hours)
  AND (
:project_id::uuid IS NULL
    OR j.project_id =:project_id
  )
GROUP BY
  b.system
ORDER BY
  build_count DESC;

--! count_evaluations
SELECT
  COUNT(*)
FROM
  evaluations;

--! evaluations_by_status
SELECT
  status::text AS status,
  COUNT(*) AS count
FROM
  evaluations
GROUP BY
  status;

--! overview_counts
SELECT
  (
    SELECT
      COUNT(*)
    FROM
      projects
  ) AS project_count,
  (
    SELECT
      COUNT(*)
    FROM
      channels
  ) AS channel_count,
  (
    SELECT
      COUNT(*)
    FROM
      remote_builders
    WHERE
      enabled = true
  ) AS builder_count;

--! per_project_build_counts
SELECT
  p.name,
  COUNT(*) FILTER (
    WHERE
      b.status = 'succeeded'
  ) AS succeeded_count,
  COUNT(*) FILTER (
    WHERE
      b.status = 'failed'
  ) AS failed_count
FROM
  builds b
  JOIN evaluations e ON b.evaluation_id = e.id
  JOIN jobsets j ON e.jobset_id = j.id
  JOIN projects p ON j.project_id = p.id
GROUP BY
  p.name;

--! duration_percentiles_overall : (duration_p50?, duration_p95?, duration_p99?)
SELECT
  PERCENTILE_CONT(0.5) WITHIN GROUP (
    ORDER BY
      EXTRACT(
        EPOCH
        FROM
          (completed_at - started_at)
      )
  ) AS duration_p50,
  PERCENTILE_CONT(0.95) WITHIN GROUP (
    ORDER BY
      EXTRACT(
        EPOCH
        FROM
          (completed_at - started_at)
      )
  ) AS duration_p95,
  PERCENTILE_CONT(0.99) WITHIN GROUP (
    ORDER BY
      EXTRACT(
        EPOCH
        FROM
          (completed_at - started_at)
      )
  ) AS duration_p99
FROM
  builds
WHERE
  completed_at IS NOT NULL
  AND started_at IS NOT NULL;
