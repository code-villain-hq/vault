-- Flagged mail (`\Flagged`), beside read: in the Inbox's copy, in the
-- kept folders, and waiting for its server (`queued.rs`) as `seen` does.
ALTER TABLE inbox_mails ADD COLUMN flagged boolean NOT NULL DEFAULT false;
ALTER TABLE folder_mails ADD COLUMN flagged boolean NOT NULL DEFAULT false;
ALTER TABLE queued_mails ADD COLUMN flagged boolean;

-- What's kept was read without its flags: the keeper reads every folder
-- once more. One never read stays so.
UPDATE kept_folders SET stamp = 'flags' WHERE stamp IS NOT NULL;
