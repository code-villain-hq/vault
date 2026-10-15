-- An issue's spec change as it was when work on the issue started, so a
-- change to it afterwards shows (decision "A spec change is written first,
-- and only one issue changes a requirement at a time"): {specs,
-- no_change_reason}. Empty specs for one written after work started.
ALTER TABLE spec_changes ADD COLUMN as_started jsonb;

UPDATE workspaces SET sync_epoch = gen_random_uuid();
