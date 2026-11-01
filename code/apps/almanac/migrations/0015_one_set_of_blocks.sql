-- One set of blocks: heading, paragraph, bullets, checklist, toggle,
-- callout and collection, beside today's kinds until pages move into the
-- set. A collection names its fields, each with its kind (text, number,
-- day, choice, status, check, photo, link), and its points are its rows:
-- a row's text is its name, its cells the rest, by field name. The app
-- picks a collection's picture from its fields; View as may pin a line, a
-- board, a week or bars too.
ALTER TABLE blocks DROP CONSTRAINT blocks_kind_check;
ALTER TABLE blocks ADD CONSTRAINT blocks_kind_check
    CHECK (kind IN ('today', 'week', 'chips', 'shelf', 'cards', 'text', 'list', 'timeline', 'you',
        'bars', 'questions', 'table', 'trend', 'parts',
        'heading', 'paragraph', 'bullets', 'checklist', 'toggle', 'callout', 'collection'));
ALTER TABLE blocks ADD COLUMN fields jsonb NOT NULL DEFAULT '[]';
ALTER TABLE blocks DROP CONSTRAINT blocks_view_check;
ALTER TABLE blocks ADD CONSTRAINT blocks_view_check
    CHECK (view IN ('', 'table', 'cards', 'list', 'lines', 'timeline', 'line', 'board', 'week',
        'bars'));

ALTER TABLE points ADD COLUMN cells jsonb NOT NULL DEFAULT '{}';

UPDATE workspaces SET sync_epoch = gen_random_uuid();
