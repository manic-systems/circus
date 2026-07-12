ALTER TABLE evaluations
DROP CONSTRAINT evaluations_status_check;

ALTER TABLE evaluations
ADD CONSTRAINT evaluations_status_check CHECK (
  status IN (
    'pending',
    'running',
    'completed',
    'failed',
    'cancelled',
    'timed_out'
  )
);
