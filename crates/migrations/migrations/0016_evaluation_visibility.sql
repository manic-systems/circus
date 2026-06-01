ALTER TABLE evaluations
ADD COLUMN hidden BOOLEAN NOT NULL DEFAULT false;

CREATE INDEX idx_evaluations_visible_time ON evaluations (evaluation_time DESC)
WHERE
  hidden = false;
