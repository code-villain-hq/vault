-- View as: how a person asked a block to be drawn, a table, cards, a list
-- (lines when it isn't steps to tick) or a timeline, which stays until they
-- pick another. Empty is the app's own pick from what the block keeps, and
-- a talk never sets it.
ALTER TABLE blocks ADD COLUMN view text NOT NULL DEFAULT ''
    CHECK (view IN ('', 'table', 'cards', 'list', 'lines', 'timeline'));

UPDATE workspaces SET sync_epoch = gen_random_uuid();
