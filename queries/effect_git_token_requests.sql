--! insert
INSERT INTO effect_git_token_requests (build_id)
VALUES (:build_id)
ON CONFLICT DO NOTHING;

--! requested
SELECT EXISTS (
  SELECT 1 FROM effect_git_token_requests WHERE build_id = :build_id
) AS requested;
