-- The palette and the default workflow follow the brand's colour meanings
-- (APP-1). Every earlier palette colour moves to the new one in the same
-- place; colours people picked outside the palette stay as they are.
CREATE TEMP TABLE palette_moves (old text PRIMARY KEY, new text NOT NULL);
INSERT INTO palette_moves (old, new) VALUES
    ('#5b5bd6', '#9fa9ff'),
    ('#4cb782', '#5bc8a6'),
    ('#f2c94c', '#e0a53b'),
    ('#f2994a', '#f2994a'),
    ('#d1453b', '#ff8b7c'),
    ('#bb87fc', '#d691f0'),
    ('#4ea7fc', '#7fb3fb'),
    ('#8a8a94', '#b0b0bd');

-- A status still on its earlier default takes its new default, by meaning
-- rather than by palette position (Done was indigo and becomes teal).
UPDATE workflow_states s SET color = d.new
FROM (VALUES
    ('Triage', '#f2994a', '#f2994a'),
    ('Backlog', '#8a8a94', '#b0b0bd'),
    ('Todo', '#b5b5bd', '#d2d2dc'),
    ('In Progress', '#f2c94c', '#7fb3fb'),
    ('In Review', '#4cb782', '#e0a53b'),
    ('Done', '#5b5bd6', '#5bc8a6'),
    ('Canceled', '#8a8a94', '#b0b0bd')
) AS d (name, old, new)
WHERE s.name = d.name AND lower(s.color) = d.old;

UPDATE services SET color = m.new FROM palette_moves m WHERE lower(services.color) = m.old;
UPDATE domains SET color = m.new FROM palette_moves m WHERE lower(domains.color) = m.old;
UPDATE labels SET color = m.new FROM palette_moves m WHERE lower(labels.color) = m.old;

ALTER TABLE workflow_states ALTER COLUMN color SET DEFAULT '#b0b0bd';
ALTER TABLE labels ALTER COLUMN color SET DEFAULT '#b0b0bd';
ALTER TABLE services ALTER COLUMN color SET DEFAULT '#b0b0bd';
ALTER TABLE domains ALTER COLUMN color SET DEFAULT '#b0b0bd';

DROP TABLE palette_moves;

UPDATE workspaces SET sync_epoch = gen_random_uuid();
