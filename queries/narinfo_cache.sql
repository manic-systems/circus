-- Project visibility includes direct and shared ownership.
--: NarinfoCacheRow(file_hash?, file_size?, deriver?, sig?, ca?, build_id?, project_id?, last_fetched_at?)

--! upsert (file_hash?, file_size?, deriver?, sig?, ca?, build_id?, project_id?)
INSERT INTO
  narinfo_cache (
    store_path, nar_hash, nar_size, file_hash, file_size, compression, url,
    deriver, "references", sig, ca, build_id, project_id, updated_at
  )
VALUES
  (
    :store_path, :nar_hash, :nar_size, :file_hash, :file_size, :compression,
    :url, :deriver, :references, :sig, :ca, :build_id, :project_id, NOW()
  )
ON CONFLICT (store_path) DO UPDATE
SET
  nar_hash = EXCLUDED.nar_hash,
  nar_size = EXCLUDED.nar_size,
  file_hash = EXCLUDED.file_hash,
  file_size = EXCLUDED.file_size,
  compression = EXCLUDED.compression,
  url = EXCLUDED.url,
  deriver = EXCLUDED.deriver,
  "references" = EXCLUDED."references",
  sig = EXCLUDED.sig,
  ca = EXCLUDED.ca,
  build_id = COALESCE(narinfo_cache.build_id, EXCLUDED.build_id),
  project_id = COALESCE(narinfo_cache.project_id, EXCLUDED.project_id),
  updated_at = NOW();

--! upsert_project_owner (build_id?)
INSERT INTO
  narinfo_cache_projects (store_path, project_id, build_id, updated_at)
VALUES
  (:store_path, :project_id, :build_id, NOW())
ON CONFLICT (store_path, project_id) DO UPDATE
SET
  build_id = COALESCE(EXCLUDED.build_id, narinfo_cache_projects.build_id),
  updated_at = NOW();

--! get : NarinfoCacheRow
SELECT * FROM narinfo_cache WHERE store_path = :store_path;

--! get_by_hash_part (project_id?) : NarinfoCacheRow
SELECT *
FROM narinfo_cache n
WHERE n.store_path LIKE :hash_part_pattern
  AND (:project_id::uuid IS NULL OR n.project_id = :project_id
    OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp
      WHERE ncp.store_path = n.store_path AND ncp.project_id = :project_id))
ORDER BY n.updated_at DESC
LIMIT 1;

--! get_by_url (project_id?) : NarinfoCacheRow
SELECT *
FROM narinfo_cache n
WHERE n.url = :url
  AND (:project_id::uuid IS NULL OR n.project_id = :project_id
    OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp
      WHERE ncp.store_path = n.store_path AND ncp.project_id = :project_id))
ORDER BY n.updated_at DESC
LIMIT 1;

--! count
SELECT COUNT(*) FROM narinfo_cache;

-- Include signed local outputs that were never uploaded, without double-counting.
--! storage_summary (project_id?) : (nar_count, uncompressed_bytes, compressed_bytes)
WITH uploaded AS (
  SELECT store_path, nar_size, file_size
  FROM narinfo_cache n
  WHERE (:project_id::uuid IS NULL OR n.project_id = :project_id
    OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp
      WHERE ncp.store_path = n.store_path AND ncp.project_id = :project_id))
),
local AS (
  SELECT DISTINCT ON (path)
    path AS store_path, COALESCE(file_size, 0) AS nar_size,
    NULL::bigint AS file_size
  FROM (
    SELECT bp.path, bp.file_size, bp.created_at
    FROM build_products bp
    JOIN builds b ON b.id = bp.build_id
    JOIN evaluations e ON e.id = b.evaluation_id
    JOIN jobsets j ON j.id = e.jobset_id
    WHERE b.status = 'succeeded' AND b.signed = true
      AND (:project_id::uuid IS NULL OR j.project_id = :project_id)
    UNION ALL
    SELECT b.build_output_path AS path, NULL::bigint AS file_size,
      COALESCE(b.completed_at, b.created_at) AS created_at
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
inventory AS (SELECT * FROM uploaded UNION ALL SELECT * FROM local)
SELECT
  COUNT(*) AS nar_count,
  COALESCE(SUM(nar_size), 0)::bigint AS uncompressed_bytes,
  COALESCE(SUM(file_size), 0)::bigint AS compressed_bytes
FROM inventory;

--! storage_extremes (project_id?) : (last_uploaded?, oldest_fetched?)
WITH uploaded AS (
  SELECT store_path, created_at, last_fetched_at
  FROM narinfo_cache n
  WHERE (:project_id::uuid IS NULL OR n.project_id = :project_id
    OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp
      WHERE ncp.store_path = n.store_path AND ncp.project_id = :project_id))
),
local AS (
  SELECT DISTINCT ON (path)
    path AS store_path, created_at, NULL::timestamptz AS last_fetched_at
  FROM (
    SELECT bp.path, bp.created_at
    FROM build_products bp
    JOIN builds b ON b.id = bp.build_id
    JOIN evaluations e ON e.id = b.evaluation_id
    JOIN jobsets j ON j.id = e.jobset_id
    WHERE b.status = 'succeeded' AND b.signed = true
      AND (:project_id::uuid IS NULL OR j.project_id = :project_id)
    UNION ALL
    SELECT b.build_output_path AS path,
      COALESCE(b.completed_at, b.created_at) AS created_at
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
inventory AS (SELECT * FROM uploaded UNION ALL SELECT * FROM local)
SELECT
  MAX(created_at) AS last_uploaded,
  MIN(last_fetched_at) AS oldest_fetched
FROM inventory;

--! list_filtered (project_id?, hash_prefix?, package_query?) : (file_size?, last_fetched_at?)
WITH uploaded AS (
  SELECT store_path, nar_size, file_size, compression, created_at,
    last_fetched_at
  FROM narinfo_cache n
  WHERE (:project_id::uuid IS NULL OR n.project_id = :project_id
    OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp
      WHERE ncp.store_path = n.store_path AND ncp.project_id = :project_id))
),
local AS (
  SELECT DISTINCT ON (path)
    path AS store_path, COALESCE(file_size, 0) AS nar_size,
    NULL::bigint AS file_size, 'none' AS compression, created_at,
    NULL::timestamptz AS last_fetched_at
  FROM (
    SELECT bp.path, bp.file_size, bp.created_at
    FROM build_products bp
    JOIN builds b ON b.id = bp.build_id
    JOIN evaluations e ON e.id = b.evaluation_id
    JOIN jobsets j ON j.id = e.jobset_id
    WHERE b.status = 'succeeded' AND b.signed = true
      AND (:project_id::uuid IS NULL OR j.project_id = :project_id)
    UNION ALL
    SELECT b.build_output_path AS path, NULL::bigint AS file_size,
      COALESCE(b.completed_at, b.created_at) AS created_at
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
inventory AS (SELECT * FROM uploaded UNION ALL SELECT * FROM local)
SELECT
  store_path,
  COALESCE(substring(store_path FROM '^/nix/store/[^-]+-(.*)$'), store_path)
    AS package_name,
  nar_size, file_size, compression, created_at, last_fetched_at
FROM inventory
WHERE (:hash_prefix::text IS NULL
    OR store_path LIKE '/nix/store/' || :hash_prefix || '%')
  AND (:package_query::text IS NULL
    OR store_path LIKE '%-%' || :package_query || '%')
ORDER BY created_at DESC
LIMIT :limit
OFFSET :offset;

--! count_filtered (project_id?, hash_prefix?, package_query?)
WITH uploaded AS (
  SELECT store_path
  FROM narinfo_cache n
  WHERE (:project_id::uuid IS NULL OR n.project_id = :project_id
    OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp
      WHERE ncp.store_path = n.store_path AND ncp.project_id = :project_id))
),
local AS (
  SELECT DISTINCT ON (path) path AS store_path
  FROM (
    SELECT bp.path, bp.created_at
    FROM build_products bp
    JOIN builds b ON b.id = bp.build_id
    JOIN evaluations e ON e.id = b.evaluation_id
    JOIN jobsets j ON j.id = e.jobset_id
    WHERE b.status = 'succeeded' AND b.signed = true
      AND (:project_id::uuid IS NULL OR j.project_id = :project_id)
    UNION ALL
    SELECT b.build_output_path AS path,
      COALESCE(b.completed_at, b.created_at) AS created_at
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
inventory AS (SELECT * FROM uploaded UNION ALL SELECT * FROM local)
SELECT COUNT(*)
FROM inventory
WHERE (:hash_prefix::text IS NULL
    OR store_path LIKE '/nix/store/' || :hash_prefix || '%')
  AND (:package_query::text IS NULL
    OR store_path LIKE '%-%' || :package_query || '%');

--! touch_last_fetched
UPDATE narinfo_cache SET last_fetched_at = NOW() WHERE store_path = :store_path;
