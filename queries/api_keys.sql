--: ApiKeyRow(user_id?, last_used_at?)
--! create : ApiKeyRow
INSERT INTO
  api_keys (name, key_hash, role)
VALUES
  (:name,:key_hash,:role)
RETURNING
  *;

--! upsert : ApiKeyRow
INSERT INTO
  api_keys (name, key_hash, role)
VALUES
  (:name,:key_hash,:role)
ON CONFLICT (key_hash) DO UPDATE
SET
  name = EXCLUDED.name,
  role = EXCLUDED.role
RETURNING
  *;

--! get_by_hash : ApiKeyRow
SELECT
  *
FROM
  api_keys
WHERE
  key_hash =:key_hash;

--! list : ApiKeyRow
SELECT
  *
FROM
  api_keys
ORDER BY
  created_at DESC;

--! delete
DELETE FROM api_keys
WHERE
  id =:id;

--! touch_last_used
UPDATE api_keys
SET
  last_used_at = NOW()
WHERE
  id =:id;
