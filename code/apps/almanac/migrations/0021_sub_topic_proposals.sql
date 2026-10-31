-- A topic's talk proposes several sub-topics in one answer, each started
-- on its own.
ALTER TABLE messages ADD COLUMN sub_topics jsonb NOT NULL DEFAULT '[]';

UPDATE workspaces SET sync_epoch = gen_random_uuid();
