--: RemoteBuilderRow(public_host_key?, ssh_key_file?, disabled_until?, last_failure?, cpu_cores?)
--! create (public_host_key?, ssh_key_file?) : RemoteBuilderRow
INSERT INTO
  remote_builders (
    name,
    ssh_uri,
    systems,
    max_jobs,
    speed_factor,
    supported_features,
    mandatory_features,
    public_host_key,
    ssh_key_file
  )
VALUES
  (
:name,
:ssh_uri,
:systems,
:max_jobs,
:speed_factor,
:supported_features,
:mandatory_features,
:public_host_key,
:ssh_key_file
  )
RETURNING
  *;

--! get : RemoteBuilderRow
SELECT
  *
FROM
  remote_builders
WHERE
  id =:id;

--! list : RemoteBuilderRow
SELECT
  *
FROM
  remote_builders
ORDER BY
  speed_factor DESC,
  name;

--! list_enabled : RemoteBuilderRow
SELECT
  *
FROM
  remote_builders
WHERE
  enabled = true
ORDER BY
  speed_factor DESC,
  name;

--! find_for_system_speed_factor : RemoteBuilderRow
SELECT
  *
FROM
  remote_builders
WHERE
  enabled = true
  AND :system = ANY (systems)
  AND (
    disabled_until IS NULL
    OR disabled_until < NOW()
  )
ORDER BY
  speed_factor DESC;

--! find_for_system_cpu_weighted : RemoteBuilderRow
SELECT
  *
FROM
  remote_builders
WHERE
  enabled = true
  AND :system = ANY (systems)
  AND (
    disabled_until IS NULL
    OR disabled_until < NOW()
  )
ORDER BY
  COALESCE(cpu_cores, 1) * speed_factor DESC;

--! find_for_system_dynamic : RemoteBuilderRow
SELECT
  r.*
FROM
  remote_builders r
  LEFT JOIN (
    SELECT
      builder_id,
      COUNT(*) AS cnt
    FROM
      builds
    WHERE
      status = 'running'
    GROUP BY
      builder_id
  ) active ON active.builder_id = r.id
WHERE
  r.enabled = true
  AND :system = ANY (r.systems)
  AND (
    r.disabled_until IS NULL
    OR r.disabled_until < NOW()
  )
ORDER BY
  (r.max_jobs - COALESCE(active.cnt, 0)) * r.speed_factor DESC;

--! record_failure : RemoteBuilderRow
UPDATE remote_builders
SET
  consecutive_failures = LEAST(consecutive_failures + 1, 4),
  last_failure = NOW(),
  disabled_until = NOW() + make_interval(
    secs => 60.0 * power(3, LEAST(consecutive_failures + 1, 4) - 1) + (random() * 30)::int
  )
WHERE
  id =:id
RETURNING
  *;

--! record_success : RemoteBuilderRow
UPDATE remote_builders
SET
  consecutive_failures = 0,
  disabled_until = NULL
WHERE
  id =:id
RETURNING
  *;

--! update (name?, ssh_uri?, systems?, max_jobs?, speed_factor?, supported_features?, mandatory_features?, enabled?, public_host_key?, ssh_key_file?) : RemoteBuilderRow
UPDATE remote_builders
SET
  name = COALESCE(:name, name),
  ssh_uri = COALESCE(:ssh_uri, ssh_uri),
  systems = COALESCE(:systems, systems),
  max_jobs = COALESCE(:max_jobs, max_jobs),
  speed_factor = COALESCE(:speed_factor, speed_factor),
  supported_features = COALESCE(:supported_features, supported_features),
  mandatory_features = COALESCE(:mandatory_features, mandatory_features),
  enabled = COALESCE(:enabled, enabled),
  public_host_key = COALESCE(:public_host_key, public_host_key),
  ssh_key_file = COALESCE(:ssh_key_file, ssh_key_file)
WHERE
  id =:id
RETURNING
  *;

--! delete
DELETE FROM remote_builders
WHERE
  id =:id;

--! count
SELECT
  COUNT(*)
FROM
  remote_builders;

--! upsert (public_host_key?, ssh_key_file?) : RemoteBuilderRow
INSERT INTO
  remote_builders (
    name,
    ssh_uri,
    systems,
    max_jobs,
    speed_factor,
    supported_features,
    mandatory_features,
    enabled,
    public_host_key,
    ssh_key_file
  )
VALUES
  (
:name,
:ssh_uri,
:systems,
:max_jobs,
:speed_factor,
:supported_features,
:mandatory_features,
:enabled,
:public_host_key,
:ssh_key_file
  )
ON CONFLICT (name) DO UPDATE
SET
  ssh_uri = EXCLUDED.ssh_uri,
  systems = EXCLUDED.systems,
  max_jobs = EXCLUDED.max_jobs,
  speed_factor = EXCLUDED.speed_factor,
  supported_features = EXCLUDED.supported_features,
  mandatory_features = EXCLUDED.mandatory_features,
  enabled = EXCLUDED.enabled,
  public_host_key = COALESCE(
    EXCLUDED.public_host_key,
    remote_builders.public_host_key
  ),
  ssh_key_file = COALESCE(
    EXCLUDED.ssh_key_file,
    remote_builders.ssh_key_file
  )
RETURNING
  *;

--! sync_all_delete
DELETE FROM remote_builders
WHERE
  name != ALL (:names::text[]);
