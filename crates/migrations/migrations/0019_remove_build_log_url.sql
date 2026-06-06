-- Remove the unused log_url column from builds as it was never populated.
ALTER TABLE builds
DROP COLUMN IF EXISTS log_url;
