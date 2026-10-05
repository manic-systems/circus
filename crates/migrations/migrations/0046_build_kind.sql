ALTER TABLE builds
ADD COLUMN kind TEXT NOT NULL DEFAULT 'build';

ALTER TABLE builds
ADD CONSTRAINT builds_kind_check CHECK (kind IN ('build', 'effect'));

CREATE INDEX idx_builds_kind ON builds (kind)
WHERE
  kind != 'build';

CREATE OR REPLACE VIEW build_stats AS
SELECT
  COUNT(*) AS total_builds,
  COUNT(
    CASE
      WHEN status = 'succeeded' THEN 1
    END
  ) AS completed_builds,
  COUNT(
    CASE
      WHEN status = 'failed' THEN 1
    END
  ) AS failed_builds,
  COUNT(
    CASE
      WHEN status = 'running' THEN 1
    END
  ) AS running_builds,
  COUNT(
    CASE
      WHEN status = 'pending' THEN 1
    END
  ) AS pending_builds,
  AVG(
    EXTRACT(
      EPOCH
      FROM
        (completed_at - started_at)
    )
  )::double precision AS avg_duration_seconds
FROM
  builds
WHERE
  kind = 'build';
