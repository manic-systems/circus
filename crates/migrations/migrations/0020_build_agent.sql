-- Record which connected agent ran a build. NULL = local or an SSH remote_builder.
ALTER TABLE builds
ADD COLUMN agent_machine_id UUID REFERENCES builder_sessions (machine_id) ON DELETE SET NULL;
