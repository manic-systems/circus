-- Per-build required features.
--
-- Builders advertise a list of supported features (e.g. "kvm", "big-parallel",
-- "nixos-test"); derivations declare which they require via the
-- `requiredSystemFeatures` Nix attribute. The scheduler must match those
-- against the candidate's `supported_features` set and skip incompatible
-- builders.
--
-- The SSH path keys off `remote_builders.mandatory_features`; this column is
-- the build-side counterpart so the agent path can do the same gate.
ALTER TABLE builds
ADD COLUMN IF NOT EXISTS required_features TEXT[] NOT NULL DEFAULT '{}';

CREATE INDEX IF NOT EXISTS idx_builds_required_features ON builds USING GIN (required_features);
