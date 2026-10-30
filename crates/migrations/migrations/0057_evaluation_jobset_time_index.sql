-- expected_durations walks a jobset's evaluations newest first instead of the
-- job's successes across every jobset.
CREATE INDEX IF NOT EXISTS idx_evaluations_jobset_time ON evaluations (jobset_id, evaluation_time DESC);

DROP INDEX IF EXISTS idx_evaluations_jobset_id;

DROP INDEX IF EXISTS idx_builds_job_succeeded;
