-- The page as a grid of twelve columns: a block takes a quarter, a third,
-- a half, two thirds, three quarters or the whole width, and empty is its
-- kind's own. Three kinds join: table, things compared across the same
-- facts; trend, a number kept each time it's measured; and parts, a whole
-- split into the parts it's made of.
ALTER TABLE blocks DROP CONSTRAINT blocks_kind_check;
ALTER TABLE blocks ADD CONSTRAINT blocks_kind_check
    CHECK (kind IN ('today', 'week', 'chips', 'shelf', 'cards', 'text', 'list', 'timeline', 'you',
        'bars', 'questions', 'table', 'trend', 'parts'));
ALTER TABLE blocks ADD COLUMN width text NOT NULL DEFAULT ''
    CHECK (width IN ('', '1/4', '1/3', '1/2', '2/3', '3/4', 'whole'));

UPDATE workspaces SET sync_epoch = gen_random_uuid();
