-- Stamp when a running evaluation was claimed so stranded rows are detectable.
ALTER TABLE evaluations
ADD COLUMN started_at TIMESTAMP WITH TIME ZONE;

-- Requeue budget for orphaned evaluations before the sweep gives up.
ALTER TABLE evaluations
ADD COLUMN orphaned_count INTEGER NOT NULL DEFAULT 0;
