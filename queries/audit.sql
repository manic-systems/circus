--: AuditLogRow(actor_id?, actor_name?, target_kind?, target_id?, remote_addr?)

--! record (actor_id?, actor_name?, target_kind?, target_id?, remote_addr?)
INSERT INTO audit_log (
  actor_kind,
  actor_id,
  actor_name,
  action,
  target_kind,
  target_id,
  details,
  remote_addr
)
VALUES (
  :actor_kind,
  :actor_id,
  :actor_name,
  :action,
  :target_kind,
  :target_id,
  :details,
  :remote_addr
);

--! list : AuditLogRow
SELECT
  id,
  occurred_at,
  actor_kind,
  actor_id,
  actor_name,
  action,
  target_kind,
  target_id,
  details,
  remote_addr
FROM
  audit_log
ORDER BY
  occurred_at DESC
LIMIT
  :limit
OFFSET
  :offset;

--! count
SELECT
  COUNT(*)
FROM
  audit_log;
