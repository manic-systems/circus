CREATE TABLE project_state_files (
  project_id UUID NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
  name TEXT NOT NULL CHECK (name ~ '^[A-Za-z0-9_-][A-Za-z0-9._-]{0,254}$'),
  data BYTEA NOT NULL,
  updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  updated_by_build UUID REFERENCES builds (id) ON DELETE SET NULL,
  PRIMARY KEY (project_id, name)
);
