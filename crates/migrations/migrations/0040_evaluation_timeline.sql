ALTER TABLE evaluations
ADD COLUMN finished_at TIMESTAMP WITH TIME ZONE,
ADD COLUMN commit_subject TEXT;

-- Many queries change evaluation status, and requeues must clear the end time.
CREATE OR REPLACE FUNCTION set_evaluation_finished_at () RETURNS trigger AS $$
BEGIN
    IF TG_OP = 'INSERT' OR OLD.status IS DISTINCT FROM NEW.status THEN
        NEW.finished_at := CASE
            WHEN NEW.status IN ('pending', 'running') THEN NULL
            ELSE NOW()
        END;
    END IF;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_evaluations_finished_at BEFORE INSERT
OR
UPDATE OF status ON evaluations FOR EACH ROW
EXECUTE FUNCTION set_evaluation_finished_at ();
