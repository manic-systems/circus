ALTER TABLE jobsets
ADD COLUMN branch_pattern TEXT,
ADD COLUMN tag_pattern TEXT;

DROP VIEW active_jobsets;

CREATE VIEW active_jobsets AS
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
  OR OLD.branch IS DISTINCT FROM NEW.branch
  OR OLD.branch_pattern IS DISTINCT FROM NEW.branch_pattern
  OR OLD.tag_pattern IS DISTINCT FROM NEW.tag_pattern
)
EXECUTE FUNCTION notify_jobsets_changed ();
