ALTER TABLE jobsets
ADD COLUMN build_timeout INTEGER CHECK (build_timeout > 0);
