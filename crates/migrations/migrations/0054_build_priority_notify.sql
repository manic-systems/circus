CREATE TRIGGER trg_builds_priority_notify
AFTER
UPDATE ON builds FOR EACH ROW WHEN (OLD.priority IS DISTINCT FROM NEW.priority)
EXECUTE FUNCTION notify_builds_changed ();
