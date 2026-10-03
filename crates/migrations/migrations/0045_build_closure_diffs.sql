CREATE TABLE build_closure_diffs (
  build_id UUID PRIMARY KEY REFERENCES builds (id) ON DELETE CASCADE,
  against_build_id UUID NOT NULL REFERENCES builds (id) ON DELETE CASCADE,
  changes JSONB NOT NULL
);

CREATE INDEX idx_build_closure_diffs_against ON build_closure_diffs (against_build_id);
