ALTER TABLE users
ADD COLUMN external_id TEXT;

CREATE UNIQUE INDEX idx_users_external_identity ON users (user_type, external_id)
WHERE
  external_id IS NOT NULL;
