-- A timeline block lists what happened when, oldest first, and a point
-- says when it happened: a year (1959), a span (1987–1993) or a day
-- (11. September 2001). Empty for the points of every other kind.
ALTER TABLE blocks DROP CONSTRAINT blocks_kind_check;
ALTER TABLE blocks ADD CONSTRAINT blocks_kind_check
    CHECK (kind IN ('today', 'week', 'chips', 'shelf', 'cards', 'text', 'list', 'timeline'));
ALTER TABLE points ADD COLUMN happened text NOT NULL DEFAULT '';

UPDATE workspaces SET sync_epoch = gen_random_uuid();
