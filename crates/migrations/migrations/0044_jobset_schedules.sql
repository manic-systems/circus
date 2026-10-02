ALTER TABLE evaluations
DROP CONSTRAINT IF EXISTS evaluations_trigger_kind_check;

ALTER TABLE evaluations
ADD CONSTRAINT evaluations_trigger_kind_check CHECK (
  trigger_kind IN ('source_change', 'manual', 'interval', 'schedule')
);

-- Scheduled runs repeat the commit their schedule was read from.
DROP INDEX IF EXISTS idx_evaluations_source_unique;

CREATE UNIQUE INDEX idx_evaluations_source_unique ON evaluations (jobset_id, commit_hash)
WHERE
  trigger_kind NOT IN ('interval', 'schedule');

CREATE TABLE jobset_schedules (
  jobset_id UUID NOT NULL REFERENCES jobsets (id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  when_spec JSONB NOT NULL,
  commit_hash VARCHAR(40) NOT NULL,
  next_due_at TIMESTAMP WITH TIME ZONE NOT NULL,
  last_fired_at TIMESTAMP WITH TIME ZONE,
  PRIMARY KEY (jobset_id, name)
);

CREATE INDEX idx_jobset_schedules_due ON jobset_schedules (next_due_at);
