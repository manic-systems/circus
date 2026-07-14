-- Explicit bigint casts keep Cornucopia from inferring numeric parameters.

--! flush
INSERT INTO cache_traffic (cache_name, requests, bytes_served)
SELECT * FROM UNNEST(:cache_names::text[], :requests::bigint[], :bytes_served::bigint[]);

--! traffic_timeseries : (bucket_time, requests, bytes)
SELECT
  to_timestamp(floor(extract(epoch FROM recorded_at) / :bucket_seconds::bigint) * :bucket_seconds::bigint) AS bucket_time,
  COALESCE(SUM(requests), 0)::bigint AS requests,
  COALESCE(SUM(bytes_served), 0)::bigint AS bytes
FROM cache_traffic
WHERE cache_name = :cache_name
  AND recorded_at > NOW() - (:window_seconds::bigint * INTERVAL '1 second')
GROUP BY bucket_time
ORDER BY bucket_time ASC;

--! storage_timeseries (project_id?) : (bucket_time, packages_added, bytes_added)
WITH uploaded AS (
  SELECT store_path, created_at, file_size
  FROM narinfo_cache n
  WHERE (:project_id::uuid IS NULL OR n.project_id = :project_id
    OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp
      WHERE ncp.store_path = n.store_path AND ncp.project_id = :project_id))
),
local AS (
  SELECT DISTINCT ON (path)
    path AS store_path, created_at, COALESCE(file_size, 0) AS file_size
  FROM (
    SELECT bp.path, bp.created_at, bp.file_size
    FROM build_products bp
    JOIN builds b ON b.id = bp.build_id
    JOIN evaluations e ON e.id = b.evaluation_id
    JOIN jobsets j ON j.id = e.jobset_id
    WHERE b.status = 'succeeded' AND b.signed = true
      AND (:project_id::uuid IS NULL OR j.project_id = :project_id)
    UNION ALL
    SELECT
      b.build_output_path AS path,
      COALESCE(b.completed_at, b.created_at) AS created_at,
      NULL::bigint AS file_size
    FROM builds b
    JOIN evaluations e ON e.id = b.evaluation_id
    JOIN jobsets j ON j.id = e.jobset_id
    WHERE b.status = 'succeeded' AND b.signed = true
      AND b.build_output_path IS NOT NULL
      AND (:project_id::uuid IS NULL OR j.project_id = :project_id)
  ) candidates
  WHERE NOT EXISTS (
    SELECT 1 FROM narinfo_cache n
    WHERE n.store_path = candidates.path
      AND (:project_id::uuid IS NULL OR n.project_id = :project_id
        OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp
          WHERE ncp.store_path = n.store_path AND ncp.project_id = :project_id))
  )
  ORDER BY path, created_at DESC
),
inventory AS (
  SELECT * FROM uploaded
  UNION ALL
  SELECT * FROM local
)
SELECT
  to_timestamp(floor(extract(epoch FROM created_at) / :bucket_seconds::bigint) * :bucket_seconds::bigint) AS bucket_time,
  COUNT(*) AS packages_added,
  COALESCE(SUM(file_size), 0)::bigint AS bytes_added
FROM inventory
WHERE created_at > NOW() - (:window_seconds::bigint * INTERVAL '1 second')
GROUP BY bucket_time
ORDER BY bucket_time ASC;

--! traffic_last_hour : (requests, bytes_served)
SELECT
  COALESCE(SUM(requests), 0)::bigint AS requests,
  COALESCE(SUM(bytes_served), 0)::bigint AS bytes_served
FROM cache_traffic
WHERE cache_name = :cache_name
  AND recorded_at > NOW() - INTERVAL '1 hour';
