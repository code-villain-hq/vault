-- A picture block: photos shown large, a point each, its photo in its
-- `photo` cell and its text a caption, which may be empty.
ALTER TABLE blocks DROP CONSTRAINT blocks_kind_check;
ALTER TABLE blocks ADD CONSTRAINT blocks_kind_check
    CHECK (kind IN ('today', 'week', 'chips', 'shelf', 'cards', 'text', 'list', 'timeline', 'you',
        'bars', 'questions', 'table', 'trend', 'parts',
        'heading', 'paragraph', 'bullets', 'checklist', 'toggle', 'callout', 'collection',
        'picture'));
