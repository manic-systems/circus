--: JobsetRow(branch?, branch_pattern?, tag_pattern?, last_checked_at?)
--: ActiveJobsetRow(branch?, branch_pattern?, tag_pattern?, last_checked_at?)

--! create (branch?, branch_pattern?, tag_pattern?) : JobsetRow
INSERT INTO jobsets (project_id, name, nix_expression, enabled, flake_mode, check_interval, trigger_mode, branch, branch_pattern, tag_pattern, scheduling_shares, state, keep_nr)
VALUES (:project_id, :name, :nix_expression, :enabled, :flake_mode, :check_interval, :trigger_mode, :branch, :branch_pattern, :tag_pattern, :scheduling_shares, :state, :keep_nr)
RETURNING *;

--! get : JobsetRow
SELECT * FROM jobsets WHERE id = :id;

--! list_for_project : JobsetRow
SELECT * FROM jobsets WHERE project_id = :project_id ORDER BY created_at DESC LIMIT :limit OFFSET :offset;

--! list_all_for_project : JobsetRow
SELECT * FROM jobsets WHERE project_id = :project_id ORDER BY created_at DESC;

--! count_for_project
SELECT COUNT(*) FROM jobsets WHERE project_id = :project_id;

--! count
SELECT COUNT(*) FROM jobsets;

--! update (branch?, branch_pattern?, tag_pattern?) : JobsetRow
UPDATE jobsets
SET name = :name, nix_expression = :nix_expression, enabled = :enabled, flake_mode = :flake_mode, check_interval = :check_interval, trigger_mode = :trigger_mode, branch = :branch, branch_pattern = :branch_pattern, tag_pattern = :tag_pattern, scheduling_shares = :scheduling_shares, state = :state, keep_nr = :keep_nr
WHERE id = :id
RETURNING *;

--! delete
DELETE FROM jobsets WHERE id = :id;

--! upsert (branch?, branch_pattern?, tag_pattern?) : JobsetRow
INSERT INTO jobsets (project_id, name, nix_expression, enabled, flake_mode, check_interval, trigger_mode, branch, branch_pattern, tag_pattern, scheduling_shares, state, keep_nr)
VALUES (:project_id, :name, :nix_expression, :enabled, :flake_mode, :check_interval, :trigger_mode, :branch, :branch_pattern, :tag_pattern, :scheduling_shares, :state, :keep_nr)
ON CONFLICT (project_id, name) DO UPDATE
SET nix_expression = EXCLUDED.nix_expression, enabled = EXCLUDED.enabled, flake_mode = EXCLUDED.flake_mode, check_interval = EXCLUDED.check_interval, trigger_mode = EXCLUDED.trigger_mode, branch = EXCLUDED.branch, branch_pattern = EXCLUDED.branch_pattern, tag_pattern = EXCLUDED.tag_pattern, scheduling_shares = EXCLUDED.scheduling_shares, state = EXCLUDED.state, keep_nr = EXCLUDED.keep_nr
RETURNING *;

--! list_active : ActiveJobsetRow
SELECT * FROM active_jobsets;

--! mark_one_shot_complete
UPDATE jobsets SET enabled = false WHERE id = :id AND state = 'one_shot';

--! update_last_checked
UPDATE jobsets SET last_checked_at = NOW() WHERE id = :id;

--! has_running_builds
SELECT COUNT(*) FROM builds b JOIN evaluations e ON b.evaluation_id = e.id WHERE e.jobset_id = :jobset_id AND b.status = 'running';

--! has_unfinished_work
SELECT COUNT(*) FROM evaluations e LEFT JOIN builds b ON b.evaluation_id = e.id WHERE e.jobset_id = :jobset_id AND (e.status IN ('pending', 'running') OR b.status IN ('pending', 'running'));

--! list_due_for_eval : ActiveJobsetRow
SELECT * FROM active_jobsets WHERE last_checked_at IS NULL OR last_checked_at < NOW() - (check_interval || ' seconds')::interval ORDER BY last_checked_at NULLS FIRST LIMIT :limit;
