ALTER TABLE builds
ADD COLUMN effect_execution_active BOOLEAN NOT NULL DEFAULT FALSE;

UPDATE builds
SET
  effect_execution_active = TRUE
WHERE
  kind = 'effect'
  AND status IN ('running', 'cancelled')
  AND agent_machine_id IS NOT NULL;

ALTER TABLE builds
ADD CONSTRAINT builds_effect_execution_active_check CHECK (
  NOT effect_execution_active
  OR (
    kind = 'effect'
    AND agent_machine_id IS NOT NULL
  )
);

CREATE INDEX idx_builds_active_effect_execution ON builds (id)
WHERE
  effect_execution_active;
