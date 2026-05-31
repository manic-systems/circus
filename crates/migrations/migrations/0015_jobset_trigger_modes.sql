-- Make the evaluator trigger policy explicit per jobset.
--
-- source_change keeps the existing behavior: webhook/manual triggers and git
-- polling only build when the source/input hash changes. interval creates a new
-- evaluation/build set on every check_interval tick, even for the same commit.
ALTER TABLE jobsets
  ADD COLUMN trigger_mode VARCHAR(50) NOT NULL DEFAULT 'source_change' CHECK (
    trigger_mode IN ('source_change', 'interval')
  );

ALTER TABLE evaluations
  ADD COLUMN trigger_kind VARCHAR(50) NOT NULL DEFAULT 'source_change' CHECK (
    trigger_kind IN ('source_change', 'manual', 'interval')
  );

-- Interval-triggered evaluations must be allowed to repeat the same commit.
-- Source-change and manual triggers remain deduplicated like the original
-- UNIQUE(jobset_id, commit_hash) constraint.
ALTER TABLE evaluations
  DROP CONSTRAINT IF EXISTS evaluations_jobset_id_commit_hash_key;

CREATE UNIQUE INDEX idx_evaluations_source_unique
  ON evaluations (jobset_id, commit_hash)
  WHERE trigger_kind <> 'interval';

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
  j.scheduling_shares,
  j.created_at,
  j.updated_at,
  j.state,
  j.last_checked_at,
  j.keep_nr,
  p.name AS project_name,
  p.repository_url AS repository_url,
  j.trigger_mode
FROM
  jobsets j
  JOIN projects p ON j.project_id = p.id
WHERE
  j.state IN ('enabled', 'one_shot', 'one_at_a_time')
  AND j.enabled = true;

DROP TRIGGER IF EXISTS trg_jobsets_update_notify ON jobsets;

CREATE TRIGGER trg_jobsets_update_notify
AFTER
UPDATE ON jobsets FOR EACH ROW WHEN (
  OLD.enabled IS DISTINCT FROM NEW.enabled
  OR OLD.state IS DISTINCT FROM NEW.state
  OR OLD.nix_expression IS DISTINCT FROM NEW.nix_expression
  OR OLD.check_interval IS DISTINCT FROM NEW.check_interval
  OR OLD.trigger_mode IS DISTINCT FROM NEW.trigger_mode
)
EXECUTE FUNCTION notify_jobsets_changed ();
