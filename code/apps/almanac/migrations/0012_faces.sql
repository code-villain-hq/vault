-- The page at a glance. A cards block names its facets, the facts every
-- card has (Licht, Wasser), each with an icon, and a point says which one
-- it's about. A point may need a look (warn, info), carry a number and a
-- few words beside it (Bars), and be done (a list's, a question's). Two
-- kinds join: bars, numbers to compare, and questions, to rehearse.
ALTER TABLE blocks DROP CONSTRAINT blocks_kind_check;
ALTER TABLE blocks ADD CONSTRAINT blocks_kind_check
    CHECK (kind IN ('today', 'week', 'chips', 'shelf', 'cards', 'text', 'list', 'timeline', 'you',
        'bars', 'questions'));
ALTER TABLE blocks ADD COLUMN facets jsonb NOT NULL DEFAULT '[]';

ALTER TABLE points ADD COLUMN facet text NOT NULL DEFAULT '';
ALTER TABLE points ADD COLUMN look text NOT NULL DEFAULT ''
    CHECK (look IN ('', 'warn', 'info'));
ALTER TABLE points ADD COLUMN amount double precision;
ALTER TABLE points ADD COLUMN note text NOT NULL DEFAULT '';
ALTER TABLE points ADD COLUMN done boolean NOT NULL DEFAULT false;

UPDATE workspaces SET sync_epoch = gen_random_uuid();
