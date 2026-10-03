ALTER TABLE builds
ALTER COLUMN max_retries
SET DEFAULT 1;

UPDATE builds
SET
  max_retries = 1
WHERE
  status IN ('pending', 'running');
