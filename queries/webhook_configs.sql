--: WebhookConfigRow(secret_hash?)
--! create (secret_hash?) : WebhookConfigRow
INSERT INTO
  webhook_configs (project_id, forge_type, secret_hash)
VALUES
  (:project_id,:forge_type,:secret_hash)
RETURNING
  *;

--! get : WebhookConfigRow
SELECT
  *
FROM
  webhook_configs
WHERE
  id =:id;

--! list_for_project : WebhookConfigRow
SELECT
  *
FROM
  webhook_configs
WHERE
  project_id =:project_id
ORDER BY
  created_at DESC;

--! get_by_project_and_forge : WebhookConfigRow
SELECT
  *
FROM
  webhook_configs
WHERE
  project_id =:project_id
  AND forge_type =:forge_type
  AND enabled = true;

--! delete
DELETE FROM webhook_configs
WHERE
  id =:id;

--! upsert (secret_hash?) : WebhookConfigRow
INSERT INTO
  webhook_configs (project_id, forge_type, secret_hash, enabled)
VALUES
  (:project_id,:forge_type,:secret_hash,:enabled)
ON CONFLICT (project_id, forge_type) DO UPDATE
SET
  secret_hash = COALESCE(EXCLUDED.secret_hash, webhook_configs.secret_hash),
  enabled = EXCLUDED.enabled
RETURNING
  *;

--! sync_for_project_delete
DELETE FROM webhook_configs
WHERE
  project_id =:project_id
  AND forge_type != ALL (:forge_types::text[]);
