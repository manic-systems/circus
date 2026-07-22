CREATE TABLE effect_completion_events (
  build_id UUID NOT NULL REFERENCES builds (id) ON DELETE CASCADE,
  retry_count INTEGER NOT NULL CHECK (retry_count >= 0),
  revision BIGINT NOT NULL DEFAULT 1 CHECK (revision > 0),
  build_snapshot JSONB NOT NULL CHECK (jsonb_typeof(build_snapshot) = 'object'),
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  acknowledged_at TIMESTAMP WITH TIME ZONE,
  PRIMARY KEY (build_id, retry_count)
);

CREATE INDEX idx_effect_completion_events_pending ON effect_completion_events (created_at, build_id, retry_count)
WHERE
  acknowledged_at IS NULL;

CREATE OR REPLACE FUNCTION enqueue_effect_completion_event () RETURNS trigger AS $$
BEGIN
  INSERT INTO effect_completion_events (
    build_id,
    retry_count,
    build_snapshot
  )
  VALUES (
    NEW.id,
    NEW.retry_count,
    to_jsonb(NEW)
  )
  ON CONFLICT (build_id, retry_count) DO UPDATE
  SET revision = effect_completion_events.revision + 1,
      build_snapshot = EXCLUDED.build_snapshot,
      updated_at = NOW(),
      acknowledged_at = NULL
  WHERE effect_completion_events.build_snapshot
    IS DISTINCT FROM EXCLUDED.build_snapshot;

  RETURN NULL;
END;
$$ LANGUAGE plpgsql;

INSERT INTO
  effect_completion_events (build_id, retry_count, build_snapshot)
SELECT
  id,
  retry_count,
  to_jsonb(builds)
FROM
  builds
WHERE
  kind = 'effect'
  AND status NOT IN ('pending', 'running')
  AND (
    status != 'cancelled'
    OR NOT effect_execution_active
  );

CREATE TRIGGER trg_effect_completion_event_insert
AFTER INSERT ON builds FOR EACH ROW WHEN (
  NEW.kind = 'effect'
  AND NEW.status NOT IN ('pending', 'running')
  AND (
    NEW.status != 'cancelled'
    OR NOT NEW.effect_execution_active
  )
)
EXECUTE FUNCTION enqueue_effect_completion_event ();

CREATE TRIGGER trg_effect_completion_event_update
AFTER
UPDATE ON builds FOR EACH ROW WHEN (
  NEW.kind = 'effect'
  AND NEW.status NOT IN ('pending', 'running')
  AND (
    NEW.status != 'cancelled'
    OR NOT NEW.effect_execution_active
  )
  AND (
    OLD.status IS DISTINCT FROM NEW.status
    OR (
      NEW.status = 'cancelled'
      AND OLD.effect_execution_active
      AND NOT NEW.effect_execution_active
    )
  )
)
EXECUTE FUNCTION enqueue_effect_completion_event ();
