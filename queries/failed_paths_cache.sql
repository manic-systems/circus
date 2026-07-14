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
