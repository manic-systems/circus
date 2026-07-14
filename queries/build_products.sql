--: BuildProductRow(sha256_hash?, file_size?, content_type?, gc_root_path?)

--! create (sha256_hash?, file_size?, content_type?) : BuildProductRow
INSERT INTO build_products (build_id, name, path, sha256_hash, file_size, content_type, is_directory)
VALUES (:build_id, :name, :path, :sha256_hash, :file_size, :content_type, :is_directory)
RETURNING *;

--! get : BuildProductRow
SELECT * FROM build_products WHERE id = :id;

--! list_for_build : BuildProductRow
SELECT * FROM build_products WHERE build_id = :build_id ORDER BY created_at ASC;

--! set_gc_root_path (gc_root_path?)
UPDATE build_products
SET gc_root_path = :gc_root_path
WHERE id = :id;

--! list_pinned : (status?, system?, gc_root_path?)
SELECT b.id AS build_id, b.job_name, b.system, b.status, b.created_at AS build_created_at,
       bp.id AS product_id, bp.name AS product_name, bp.path, bp.gc_root_path,
       bp.created_at AS product_created_at
FROM builds b
JOIN build_products bp ON bp.build_id = b.id
WHERE b.keep = true
ORDER BY b.created_at DESC, bp.created_at ASC
LIMIT :limit OFFSET :offset;

--! count_pinned
SELECT COUNT(*) FROM builds b JOIN build_products bp ON bp.build_id = b.id WHERE b.keep = true;

--! list_pinned_for_gc : (status?, system?, gc_root_path?)
SELECT b.id AS build_id, b.job_name, b.system, b.status, b.created_at AS build_created_at,
       bp.id AS product_id, bp.name AS product_name, bp.path, bp.gc_root_path,
       bp.created_at AS product_created_at
FROM builds b
JOIN build_products bp ON bp.build_id = b.id
WHERE b.keep = true
ORDER BY b.created_at DESC, bp.created_at ASC;
