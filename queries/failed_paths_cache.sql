--! is_cached_failure
SELECT
  true AS exists
FROM
  failed_paths_cache
WHERE
  drv_path =:drv_path;

--! insert (source_build_id?, failure_status?)
INSERT INTO
  failed_paths_cache (
    drv_path,
    source_build_id,
    failure_status,
    failed_at
  )
VALUES
  (:drv_path,:source_build_id,:failure_status, NOW())
ON CONFLICT (drv_path) DO UPDATE
SET
  source_build_id =:source_build_id,
  failure_status =:failure_status,
  failed_at = NOW();

--! invalidate
DELETE FROM failed_paths_cache
WHERE
  drv_path =:drv_path;

--! cleanup_expired
DELETE FROM failed_paths_cache
WHERE
  failed_at < NOW() - make_interval(secs =>:ttl_seconds);
--: FailedPathsCacheClearResult(deleted, restarted)
--! clear_all
WITH cleared AS (
  DELETE FROM failed_paths_cache
  RETURNING drv_path
), restarted AS (
  UPDATE builds AS b
  SET status = 'pending',
      started_at = NULL,
      completed_at = NULL,
      log_path = NULL,
      build_output_path = NULL,
      error_message = NULL,
      started_notified_at = NULL,
      effective_features = NULL,
      retry_count = retry_count + 1
  WHERE b.status = 'cached_failure'
    AND (
      b.drv_path IN (SELECT drv_path FROM cleared)
      OR NOT EXISTS (SELECT 1 FROM cleared)
    )
  RETURNING b.id
)
SELECT
  (SELECT COUNT(*)::bigint FROM cleared) AS deleted,
  (SELECT COUNT(*)::bigint FROM restarted) AS restarted;
