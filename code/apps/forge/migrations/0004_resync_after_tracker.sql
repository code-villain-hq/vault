-- Migration 0003 created teams and workflow states and changed issues
-- directly, outside the sync log. A new epoch makes every client download a
-- fresh snapshot instead of following the log from where it was.
--
-- Any migration that changes synced records directly must do the same.
UPDATE workspaces SET sync_epoch = gen_random_uuid();
