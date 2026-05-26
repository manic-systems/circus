-- circus database schema.
-- Full schema definition for the circus system.
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- projects: stores repository configurations
CREATE TABLE projects (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  name VARCHAR(255) NOT NULL UNIQUE,
  description TEXT,
  repository_url TEXT NOT NULL,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- users: accounts for authentication and personalization
CREATE TABLE users (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  username VARCHAR(255) NOT NULL UNIQUE,
  email VARCHAR(255) NOT NULL UNIQUE,
  full_name VARCHAR(255),
  password_hash VARCHAR(255),
  user_type VARCHAR(50) NOT NULL DEFAULT 'local',
  role VARCHAR(50) NOT NULL DEFAULT 'read-only',
  enabled BOOLEAN NOT NULL DEFAULT true,
  email_verified BOOLEAN NOT NULL DEFAULT false,
  public_dashboard BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  last_login_at TIMESTAMP WITH TIME ZONE
);

-- remote_builders: multi-machine / multi-arch build agents
CREATE TABLE remote_builders (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  name VARCHAR(255) NOT NULL UNIQUE,
  ssh_uri TEXT NOT NULL,
  systems TEXT[] NOT NULL DEFAULT '{}',
  max_jobs INTEGER NOT NULL DEFAULT 1,
  speed_factor INTEGER NOT NULL DEFAULT 1,
  supported_features TEXT[] NOT NULL DEFAULT '{}',
  mandatory_features TEXT[] NOT NULL DEFAULT '{}',
  enabled BOOLEAN NOT NULL DEFAULT true,
  public_host_key TEXT,
  ssh_key_file TEXT,
  consecutive_failures INTEGER NOT NULL DEFAULT 0,
  disabled_until TIMESTAMP WITH TIME ZONE,
  last_failure TIMESTAMP WITH TIME ZONE,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- jobsets: build configurations for each project
CREATE TABLE jobsets (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  project_id UUID NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
  name VARCHAR(255) NOT NULL,
  nix_expression TEXT NOT NULL,
  enabled BOOLEAN NOT NULL DEFAULT true,
  flake_mode BOOLEAN NOT NULL DEFAULT true,
  check_interval INTEGER NOT NULL DEFAULT 60,
  branch VARCHAR(255),
  scheduling_shares INTEGER NOT NULL DEFAULT 100,
  state VARCHAR(50) NOT NULL DEFAULT 'enabled' CHECK (
    state IN (
      'disabled',
      'enabled',
      'one_shot',
      'one_at_a_time'
    )
  ),
  last_checked_at TIMESTAMP WITH TIME ZONE,
  keep_nr INTEGER NOT NULL DEFAULT 3,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  UNIQUE (project_id, name)
);

-- api_keys: authentication tokens with role-based access control
CREATE TABLE api_keys (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  name VARCHAR(255) NOT NULL,
  key_hash VARCHAR(128) NOT NULL UNIQUE,
  role VARCHAR(50) NOT NULL DEFAULT 'read-only' CHECK (
    role IN (
      'admin',
      'create-projects',
      'restart-jobs',
      'cancel-build',
      'bump-to-front',
      'eval-jobset',
      'read-only'
    )
  ),
  user_id UUID REFERENCES users (id) ON DELETE SET NULL,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  last_used_at TIMESTAMP WITH TIME ZONE
);

-- evaluations: Nix evaluation results for each jobset commit
CREATE TABLE evaluations (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  jobset_id UUID NOT NULL REFERENCES jobsets (id) ON DELETE CASCADE,
  commit_hash VARCHAR(40) NOT NULL,
  evaluation_time TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  status TEXT NOT NULL CHECK (
    status IN ('pending', 'running', 'completed', 'failed')
  ),
  error_message TEXT,
  inputs_hash VARCHAR(128),
  pr_number INTEGER,
  pr_head_branch TEXT,
  pr_base_branch TEXT,
  pr_action TEXT,
  UNIQUE (jobset_id, commit_hash)
);

-- builds: individual build jobs
CREATE TABLE builds (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  evaluation_id UUID NOT NULL REFERENCES evaluations (id) ON DELETE CASCADE,
  job_name VARCHAR(255) NOT NULL,
  drv_path TEXT NOT NULL,
  status TEXT NOT NULL CHECK (
    status IN (
      'pending',
      'running',
      'succeeded',
      'failed',
      'dependency_failed',
      'aborted',
      'cancelled',
      'failed_with_output',
      'timeout',
      'cached_failure',
      'unsupported_system',
      'log_limit_exceeded',
      'nar_size_limit_exceeded',
      'non_deterministic'
    )
  ),
  started_at TIMESTAMP WITH TIME ZONE,
  completed_at TIMESTAMP WITH TIME ZONE,
  log_path TEXT,
  build_output_path TEXT,
  error_message TEXT,
  priority INTEGER NOT NULL DEFAULT 0,
  retry_count INTEGER NOT NULL DEFAULT 0,
  max_retries INTEGER NOT NULL DEFAULT 3,
  notification_pending_since TIMESTAMP WITH TIME ZONE,
  log_url TEXT,
  outputs JSONB,
  is_aggregate BOOLEAN NOT NULL DEFAULT false,
  constituents JSONB,
  builder_id UUID REFERENCES remote_builders (id),
  signed BOOLEAN NOT NULL DEFAULT false,
  system VARCHAR(50),
  keep BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  UNIQUE (evaluation_id, job_name)
);

-- build_outputs: normalized output storage
CREATE TABLE build_outputs (
  build UUID NOT NULL REFERENCES builds (id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  path TEXT,
  PRIMARY KEY (build, name)
);

CREATE INDEX idx_build_outputs_path ON build_outputs USING btree (path);

-- build_products: output artifacts and metadata
CREATE TABLE build_products (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  build_id UUID NOT NULL REFERENCES builds (id) ON DELETE CASCADE,
  name VARCHAR(255) NOT NULL,
  path TEXT NOT NULL,
  sha256_hash VARCHAR(64),
  file_size BIGINT,
  content_type VARCHAR(100),
  is_directory BOOLEAN NOT NULL DEFAULT false,
  gc_root_path TEXT,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- build_steps: detailed build execution logs and timing
CREATE TABLE build_steps (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  build_id UUID NOT NULL REFERENCES builds (id) ON DELETE CASCADE,
  step_number INTEGER NOT NULL,
  command TEXT NOT NULL,
  output TEXT,
  error_output TEXT,
  started_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  completed_at TIMESTAMP WITH TIME ZONE,
  exit_code INTEGER,
  UNIQUE (build_id, step_number)
);

-- build_dependencies: tracks inter-build dependency relationships
CREATE TABLE build_dependencies (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  build_id UUID NOT NULL REFERENCES builds (id) ON DELETE CASCADE,
  dependency_build_id UUID NOT NULL REFERENCES builds (id) ON DELETE CASCADE,
  UNIQUE (build_id, dependency_build_id)
);

-- webhook_configs: incoming push event configuration per project
CREATE TABLE webhook_configs (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  project_id UUID NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
  forge_type VARCHAR(50) NOT NULL CHECK (
    forge_type IN ('github', 'gitea', 'forgejo', 'gitlab')
  ),
  secret_hash VARCHAR(128),
  enabled BOOLEAN NOT NULL DEFAULT true,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  UNIQUE (project_id, forge_type)
);

-- notification_configs: outgoing notification configuration per project
CREATE TABLE notification_configs (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  project_id UUID NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
  notification_type VARCHAR(50) NOT NULL CHECK (
    notification_type IN (
      'github_status',
      'gitea_status',
      'forgejo_status',
      'gitlab_status',
      'webhook',
      'email'
    )
  ),
  config JSONB NOT NULL DEFAULT '{}',
  enabled BOOLEAN NOT NULL DEFAULT true,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  UNIQUE (project_id, notification_type)
);

-- jobset_inputs: parameterized inputs for jobsets
CREATE TABLE jobset_inputs (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  jobset_id UUID NOT NULL REFERENCES jobsets (id) ON DELETE CASCADE,
  name VARCHAR(255) NOT NULL,
  input_type VARCHAR(50) NOT NULL CHECK (
    input_type IN ('git', 'string', 'boolean', 'path', 'build')
  ),
  value TEXT NOT NULL,
  revision TEXT,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  UNIQUE (jobset_id, name)
);

-- channels: release management, tracks the latest good evaluation per jobset
CREATE TABLE channels (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  project_id UUID NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
  name VARCHAR(255) NOT NULL,
  jobset_id UUID NOT NULL REFERENCES jobsets (id) ON DELETE CASCADE,
  current_evaluation_id UUID REFERENCES evaluations (id),
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  UNIQUE (project_id, name)
);

-- starred_jobs: personalized dashboard bookmarks per user
CREATE TABLE starred_jobs (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  project_id UUID NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
  jobset_id UUID REFERENCES jobsets (id) ON DELETE CASCADE,
  job_name VARCHAR(255) NOT NULL,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  UNIQUE (user_id, project_id, jobset_id, job_name)
);

-- user_sessions: persistent authentication tokens
CREATE TABLE user_sessions (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  session_token_hash VARCHAR(255) NOT NULL,
  expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  last_used_at TIMESTAMP WITH TIME ZONE
);

-- project_members: per-project permission assignments
CREATE TABLE project_members (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  project_id UUID NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
  user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  role VARCHAR(50) NOT NULL DEFAULT 'member',
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  UNIQUE (project_id, user_id)
);

-- build_metrics: timing, size, and performance metrics per build
CREATE TABLE build_metrics (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  build_id UUID NOT NULL REFERENCES builds (id) ON DELETE CASCADE,
  metric_name VARCHAR(100) NOT NULL,
  metric_value DOUBLE PRECISION NOT NULL,
  unit VARCHAR(50) NOT NULL,
  collected_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  UNIQUE (build_id, metric_name)
);

-- failed_paths_cache: prevents rebuilding known-failing derivations
CREATE TABLE failed_paths_cache (
  drv_path TEXT PRIMARY KEY,
  source_build_id UUID,
  failure_status TEXT,
  failed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Indexes: projects
CREATE INDEX idx_projects_name ON projects (name);

CREATE INDEX idx_projects_created_at ON projects (created_at);

-- Indexes: users
CREATE INDEX idx_users_username ON users (username);

CREATE INDEX idx_users_email ON users (email);

CREATE INDEX idx_users_role ON users (role);

CREATE INDEX idx_users_enabled ON users (enabled);

-- Indexes: remote_builders
CREATE INDEX idx_remote_builders_enabled ON remote_builders (enabled)
WHERE
  enabled = true;

-- Indexes: jobsets
CREATE INDEX idx_jobsets_project_id ON jobsets (project_id);

CREATE INDEX idx_jobsets_enabled ON jobsets (enabled);

CREATE INDEX idx_jobsets_name ON jobsets (name);

CREATE INDEX idx_jobsets_state ON jobsets (state);

CREATE INDEX idx_jobsets_last_checked_at ON jobsets (last_checked_at);

-- Indexes: api_keys
CREATE INDEX idx_api_keys_key_hash ON api_keys (key_hash);

CREATE INDEX idx_api_keys_user_id ON api_keys (user_id);

-- Indexes: evaluations
CREATE INDEX idx_evaluations_jobset_id ON evaluations (jobset_id);

CREATE INDEX idx_evaluations_commit_hash ON evaluations (commit_hash);

CREATE INDEX idx_evaluations_status ON evaluations (status);

CREATE INDEX idx_evaluations_evaluation_time ON evaluations (evaluation_time);

CREATE INDEX idx_evaluations_inputs_hash ON evaluations (jobset_id, inputs_hash);

CREATE INDEX idx_evaluations_pr ON evaluations (jobset_id, pr_number)
WHERE
  pr_number IS NOT NULL;

-- Indexes: builds
CREATE INDEX idx_builds_evaluation_id ON builds (evaluation_id);

CREATE INDEX idx_builds_status ON builds (status);

CREATE INDEX idx_builds_job_name ON builds (job_name);

CREATE INDEX idx_builds_started_at ON builds (started_at);

CREATE INDEX idx_builds_completed_at ON builds (completed_at);

CREATE INDEX idx_builds_priority ON builds (priority DESC, created_at ASC);

CREATE INDEX idx_builds_notification_pending ON builds (notification_pending_since)
WHERE
  notification_pending_since IS NOT NULL;

CREATE INDEX idx_builds_drv_path ON builds (drv_path);

CREATE INDEX idx_builds_builder ON builds (builder_id)
WHERE
  builder_id IS NOT NULL;

CREATE INDEX idx_builds_system ON builds (system)
WHERE
  system IS NOT NULL;

CREATE INDEX idx_builds_pending_priority ON builds (status, priority DESC, created_at ASC)
WHERE
  status = 'pending';

CREATE INDEX idx_builds_drv_completed ON builds (drv_path)
WHERE
  status = 'succeeded';

-- Indexes: build_products
CREATE INDEX idx_build_products_build_id ON build_products (build_id);

CREATE INDEX idx_build_products_name ON build_products (name);

CREATE INDEX idx_build_products_path_prefix ON build_products (path text_pattern_ops);

-- Indexes: build_steps
CREATE INDEX idx_build_steps_build_id ON build_steps (build_id);

CREATE INDEX idx_build_steps_started_at ON build_steps (started_at);

-- Indexes: build_dependencies
CREATE INDEX idx_build_deps_build ON build_dependencies (build_id);

CREATE INDEX idx_build_deps_dep ON build_dependencies (dependency_build_id);

-- Indexes: webhook/notification/jobset_inputs/channels
CREATE INDEX idx_webhook_configs_project ON webhook_configs (project_id);

CREATE INDEX idx_notification_configs_project ON notification_configs (project_id);

CREATE INDEX idx_jobset_inputs_jobset ON jobset_inputs (jobset_id);

CREATE INDEX idx_channels_project ON channels (project_id);

CREATE INDEX idx_channels_jobset ON channels (jobset_id);

-- Indexes: users/sessions/members
CREATE INDEX idx_starred_jobs_user_id ON starred_jobs (user_id);

CREATE INDEX idx_starred_jobs_project_id ON starred_jobs (project_id);

CREATE INDEX idx_user_sessions_token ON user_sessions (session_token_hash);

CREATE INDEX idx_user_sessions_user_id ON user_sessions (user_id);

CREATE INDEX idx_user_sessions_expires ON user_sessions (expires_at);

CREATE INDEX idx_project_members_project_id ON project_members (project_id);

CREATE INDEX idx_project_members_user_id ON project_members (user_id);

-- Indexes: build_metrics / failed_paths_cache
CREATE INDEX idx_build_metrics_build_id ON build_metrics (build_id);

CREATE INDEX idx_build_metrics_collected_at ON build_metrics (collected_at);

CREATE INDEX idx_build_metrics_name ON build_metrics (metric_name);

CREATE INDEX idx_failed_paths_cache_failed_at ON failed_paths_cache (failed_at);

-- Trigger function: auto-update updated_at on mutation
CREATE OR REPLACE FUNCTION update_updated_at_column () RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_projects_updated_at BEFORE
UPDATE ON projects FOR EACH ROW
EXECUTE FUNCTION update_updated_at_column ();

CREATE TRIGGER update_jobsets_updated_at BEFORE
UPDATE ON jobsets FOR EACH ROW
EXECUTE FUNCTION update_updated_at_column ();

CREATE TRIGGER update_users_updated_at BEFORE
UPDATE ON users FOR EACH ROW
EXECUTE FUNCTION update_updated_at_column ();

-- Trigger functions: LISTEN/NOTIFY for event-driven daemon wakeup
CREATE OR REPLACE FUNCTION notify_builds_changed () RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('circus_builds_changed', json_build_object(
        'op', TG_OP,
        'table', TG_TABLE_NAME
    )::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION notify_jobsets_changed () RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('circus_jobsets_changed', json_build_object(
        'op', TG_OP,
        'table', TG_TABLE_NAME
    )::text);
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_builds_insert_notify
AFTER INSERT ON builds FOR EACH ROW
EXECUTE FUNCTION notify_builds_changed ();

CREATE TRIGGER trg_builds_status_notify
AFTER
UPDATE ON builds FOR EACH ROW WHEN (OLD.status IS DISTINCT FROM NEW.status)
EXECUTE FUNCTION notify_builds_changed ();

CREATE TRIGGER trg_jobsets_insert_notify
AFTER INSERT ON jobsets FOR EACH ROW
EXECUTE FUNCTION notify_jobsets_changed ();

CREATE TRIGGER trg_jobsets_update_notify
AFTER
UPDATE ON jobsets FOR EACH ROW WHEN (
  OLD.enabled IS DISTINCT FROM NEW.enabled
  OR OLD.state IS DISTINCT FROM NEW.state
  OR OLD.nix_expression IS DISTINCT FROM NEW.nix_expression
  OR OLD.check_interval IS DISTINCT FROM NEW.check_interval
)
EXECUTE FUNCTION notify_jobsets_changed ();

CREATE TRIGGER trg_jobsets_delete_notify
AFTER DELETE ON jobsets FOR EACH ROW
EXECUTE FUNCTION notify_jobsets_changed ();

-- notification_tasks: persistent notification retry queue
-- Stores notification delivery tasks with automatic retry and exponential backoff
CREATE TABLE notification_tasks (
  id UUID PRIMARY KEY DEFAULT uuid_generate_v4 (),
  notification_type VARCHAR(50) NOT NULL CHECK (
    notification_type IN (
      'webhook',
      'github_status',
      'gitea_status',
      'gitlab_status',
      'email'
    )
  ),
  payload JSONB NOT NULL,
  status VARCHAR(20) NOT NULL DEFAULT 'pending' CHECK (
    status IN ('pending', 'running', 'completed', 'failed')
  ),
  attempts INTEGER NOT NULL DEFAULT 0,
  max_attempts INTEGER NOT NULL DEFAULT 5,
  next_retry_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  last_error TEXT,
  created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
  completed_at TIMESTAMP WITH TIME ZONE
);

-- Indexes: notification_tasks
CREATE INDEX idx_notification_tasks_status_next_retry ON notification_tasks (status, next_retry_at)
WHERE
  status IN ('pending', 'running');

CREATE INDEX idx_notification_tasks_created_at ON notification_tasks (created_at);

-- Views
CREATE VIEW active_jobsets AS
SELECT
  j.id,
  j.project_id,
  j.name,
  j.nix_expression,
  j.enabled,
  j.flake_mode,
  j.check_interval,
  j.branch,
  j.scheduling_shares,
  j.created_at,
  j.updated_at,
  j.state,
  j.last_checked_at,
  j.keep_nr,
  p.name as project_name,
  p.repository_url
FROM
  jobsets j
  JOIN projects p ON j.project_id = p.id
WHERE
  j.state IN ('enabled', 'one_shot', 'one_at_a_time');

CREATE VIEW build_stats AS
SELECT
  COUNT(*) as total_builds,
  COUNT(
    CASE
      WHEN status = 'succeeded' THEN 1
    END
  ) as completed_builds,
  COUNT(
    CASE
      WHEN status = 'failed' THEN 1
    END
  ) as failed_builds,
  COUNT(
    CASE
      WHEN status = 'running' THEN 1
    END
  ) as running_builds,
  COUNT(
    CASE
      WHEN status = 'pending' THEN 1
    END
  ) as pending_builds,
  AVG(
    EXTRACT(
      EPOCH
      FROM
        (completed_at - started_at)
    )
  )::double precision as avg_duration_seconds
FROM
  builds
WHERE
  started_at IS NOT NULL;

CREATE VIEW build_metrics_summary AS
SELECT
  b.id as build_id,
  b.job_name,
  b.status,
  b.system,
  e.jobset_id,
  j.project_id,
  b.started_at,
  b.completed_at,
  EXTRACT(
    EPOCH
    FROM
      (b.completed_at - b.started_at)
  ) as duration_seconds,
  MAX(
    CASE
      WHEN bm.metric_name = 'output_size_bytes' THEN bm.metric_value
    END
  ) as output_size_bytes,
  MAX(
    CASE
      WHEN bm.metric_name = 'peak_memory_bytes' THEN bm.metric_value
    END
  ) as peak_memory_bytes,
  MAX(
    CASE
      WHEN bm.metric_name = 'nar_size_bytes' THEN bm.metric_value
    END
  ) as nar_size_bytes
FROM
  builds b
  JOIN evaluations e ON b.evaluation_id = e.id
  JOIN jobsets j ON e.jobset_id = j.id
  LEFT JOIN build_metrics bm ON b.id = bm.build_id
GROUP BY
  b.id,
  b.job_name,
  b.status,
  b.system,
  e.jobset_id,
  j.project_id,
  b.started_at,
  b.completed_at;
