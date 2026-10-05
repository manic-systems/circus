-- Effects whose secretsMap asks for a forge-minted GitToken.
CREATE TABLE effect_git_token_requests (
  build_id UUID PRIMARY KEY REFERENCES builds (id) ON DELETE CASCADE
);
