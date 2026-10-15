-- A spec change that took effect is part of its specs' history, so it stays
-- when its issue is deleted, without the issue. The engine clears issue_id
-- before deleting the issue; one that never took effect still goes with it.
ALTER TABLE spec_changes ALTER COLUMN issue_id DROP NOT NULL;
