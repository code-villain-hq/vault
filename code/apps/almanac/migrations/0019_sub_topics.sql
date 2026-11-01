-- A topic can sit under another, one level deep: Shoes under Finding
-- prices. No foreign key, as with a block's row_id: deleting the parent
-- clears it on its children through the sync engine's cascade, so every
-- client hears of it.
ALTER TABLE topics ADD COLUMN parent_id uuid;
CREATE INDEX topics_parent_idx ON topics (parent_id) WHERE parent_id IS NOT NULL;

UPDATE workspaces SET sync_epoch = gen_random_uuid();
