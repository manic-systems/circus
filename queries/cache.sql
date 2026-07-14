-- Never rebroadcast local store paths without Circus provenance.

--! has_circus_build_product (project_id?)
SELECT
  EXISTS(
    SELECT
      1
    FROM
      build_products bp
      JOIN builds b ON b.id = bp.build_id
      JOIN evaluations e ON e.id = b.evaluation_id
      JOIN jobsets j ON j.id = e.jobset_id
    WHERE
      bp.path = :store_path
      AND (
:project_id::uuid IS NULL
        OR j.project_id = :project_id
      )
    UNION ALL
    SELECT
      1
    FROM
      builds b
      JOIN evaluations e ON e.id = b.evaluation_id
      JOIN jobsets j ON j.id = e.jobset_id
    WHERE
      b.build_output_path = :store_path
      AND (
:project_id::uuid IS NULL
        OR j.project_id = :project_id
      )
  );

--! signed_narinfo_sig (project_id?)
SELECT
  nar_hash,
  nar_size,
  "references",
  sig
FROM
  narinfo_cache n
WHERE
  n.store_path = :store_path
  AND n.sig IS NOT NULL
  AND btrim(n.sig) != ''
  AND (
:project_id::uuid IS NULL
    OR n.project_id = :project_id
    OR EXISTS (
      SELECT 1 FROM narinfo_cache_projects ncp
      WHERE ncp.store_path = n.store_path AND ncp.project_id = :project_id
    )
  );

--! has_circus_signed_build_product (project_id?)
SELECT
  EXISTS(
    SELECT
      1
    FROM
      build_products bp
      JOIN builds b ON b.id = bp.build_id
      JOIN evaluations e ON e.id = b.evaluation_id
      JOIN jobsets j ON j.id = e.jobset_id
    WHERE
      bp.path = :store_path
      AND b.signed = true
      AND (
:project_id::uuid IS NULL
        OR j.project_id = :project_id
      )
    UNION ALL
    SELECT
      1
    FROM
      builds b
      JOIN evaluations e ON e.id = b.evaluation_id
      JOIN jobsets j ON j.id = e.jobset_id
    WHERE
      b.build_output_path = :store_path
      AND b.signed = true
      AND (
:project_id::uuid IS NULL
        OR j.project_id = :project_id
      )
  );

--! has_circus_derivation_path (project_id?)
SELECT
  EXISTS(
    SELECT
      1
    FROM
      builds b
      JOIN evaluations e ON e.id = b.evaluation_id
      JOIN jobsets j ON j.id = e.jobset_id
    WHERE
      b.drv_path = :store_path
      AND (
:project_id::uuid IS NULL
        OR j.project_id = :project_id
      )
  );

--! has_circus_derivation_path_any (project_id?)
SELECT
  EXISTS(
    SELECT
      1
    FROM
      builds b
      JOIN evaluations e ON e.id = b.evaluation_id
      JOIN jobsets j ON j.id = e.jobset_id
    WHERE
      b.drv_path = ANY(:drv_paths)
      AND (
:project_id::uuid IS NULL
        OR j.project_id = :project_id
      )
  );
