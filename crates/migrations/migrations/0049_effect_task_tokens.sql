CREATE TABLE effect_task_tokens (
  build_id UUID PRIMARY KEY REFERENCES builds (id) ON DELETE CASCADE,
  retry_count INTEGER NOT NULL CHECK (retry_count >= 0),
  token_hash TEXT NOT NULL UNIQUE,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);
