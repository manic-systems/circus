--! record (version?)
INSERT INTO service_heartbeats (
  service,
  last_heartbeat_at,
  poll_interval_seconds,
  version
)
VALUES (
  :service,
  NOW(),
  :poll_interval_seconds,
  :version
)
ON CONFLICT (service) DO UPDATE
SET
  last_heartbeat_at = EXCLUDED.last_heartbeat_at,
  poll_interval_seconds = EXCLUDED.poll_interval_seconds,
  version = EXCLUDED.version;

--! list_status : (version?)
SELECT
  service,
  last_heartbeat_at,
  EXTRACT(
    EPOCH
    FROM
      (NOW() - last_heartbeat_at)
  )::float8 AS seconds_since,
  poll_interval_seconds,
  version
FROM
  service_heartbeats;
