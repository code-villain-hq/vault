-- A collection can be drawn as figures, a tile a row, and as steps, its
-- rows numbered in their order; and as a checklist, which View as offers
-- since lists became collections.
ALTER TABLE blocks DROP CONSTRAINT blocks_view_check;
ALTER TABLE blocks ADD CONSTRAINT blocks_view_check
    CHECK (view IN ('', 'table', 'cards', 'list', 'lines', 'timeline', 'line', 'board', 'week',
        'bars', 'checklist', 'figures', 'steps'));
