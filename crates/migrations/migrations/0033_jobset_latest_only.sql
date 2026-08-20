ALTER TABLE jobsets
ADD COLUMN only_build_latest BOOLEAN NOT NULL DEFAULT false;

ALTER TABLE evaluations
ADD COLUMN source_scope TEXT,
ADD COLUMN superseded_by UUID REFERENCES evaluations (id) ON DELETE SET NULL;

CREATE TABLE evaluation_source_heads (
  jobset_id UUID NOT NULL REFERENCES jobsets (id) ON DELETE CASCADE,
  source_scope TEXT NOT NULL,
  commit_hash VARCHAR(40) NOT NULL,
  PRIMARY KEY (jobset_id, source_scope)
);

CREATE INDEX idx_evaluations_active_source_scope
ON evaluations (jobset_id, source_scope, evaluation_time)
WHERE trigger_kind = 'source_change';

CREATE OR REPLACE VIEW active_jobsets AS
SELECT
  j.id,
  j.project_id,
  j.name,
  j.nix_expression,
  j.enabled,
  j.flake_mode,
  j.check_interval,
  j.branch,
  j.branch_pattern,
  j.tag_pattern,
  j.scheduling_shares,
  j.created_at,
  j.updated_at,
  j.state,
  j.last_checked_at,
  j.keep_nr,
  p.name AS project_name,
  p.repository_url AS repository_url,
  j.trigger_mode,
  j.systems,
  j.only_build_latest
FROM jobsets j
JOIN projects p ON j.project_id = p.id
WHERE j.state IN ('enabled', 'one_shot', 'one_at_a_time')
  AND j.enabled = true;

DROP TRIGGER IF EXISTS trg_jobsets_update_notify ON jobsets;

CREATE TRIGGER trg_jobsets_update_notify
AFTER UPDATE ON jobsets FOR EACH ROW WHEN (
  OLD.enabled IS DISTINCT FROM NEW.enabled
  OR OLD.state IS DISTINCT FROM NEW.state
  OR OLD.nix_expression IS DISTINCT FROM NEW.nix_expression
  OR OLD.check_interval IS DISTINCT FROM NEW.check_interval
  OR OLD.trigger_mode IS DISTINCT FROM NEW.trigger_mode
  OR OLD.branch IS DISTINCT FROM NEW.branch
  OR OLD.branch_pattern IS DISTINCT FROM NEW.branch_pattern
  OR OLD.tag_pattern IS DISTINCT FROM NEW.tag_pattern
  OR OLD.systems IS DISTINCT FROM NEW.systems
  OR OLD.only_build_latest IS DISTINCT FROM NEW.only_build_latest
)
EXECUTE FUNCTION notify_jobsets_changed ();
