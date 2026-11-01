-- A row's page is made of the same blocks as a topic's: a block with a
-- row_id sits on that row's page, not on the topic's. No foreign key, as
-- with a reminder's point_id: deleting a point takes its blocks through
-- the sync engine's cascade, so every client hears of it.
ALTER TABLE blocks ADD COLUMN row_id uuid;
CREATE INDEX blocks_row_idx ON blocks (row_id) WHERE row_id IS NOT NULL;

UPDATE workspaces SET sync_epoch = gen_random_uuid();
