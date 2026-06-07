-- requiredSystemFeatures unioned over the drvs a dispatch-time
-- `nix-store --realise --dry-run` says will be built
ALTER TABLE builds
ADD COLUMN IF NOT EXISTS effective_features TEXT[];
