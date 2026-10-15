-- Each service gets an icon and a colour of its own, so it's recognisable
-- wherever it shows up. Existing services get the palette's colours in the
-- order they were made, so they don't all look the same.
ALTER TABLE services ADD COLUMN icon text NOT NULL DEFAULT 'server';
ALTER TABLE services ADD COLUMN color text NOT NULL DEFAULT '#8a8a94';

UPDATE services s
SET color = (ARRAY['#5b5bd6', '#4cb782', '#f2c94c', '#f2994a', '#d1453b', '#bb87fc', '#4ea7fc', '#8a8a94'])
    [((o.n - 1) % 8) + 1]
FROM (SELECT id, row_number() OVER (PARTITION BY workspace_id ORDER BY created_at, id) AS n FROM services) o
WHERE o.id = s.id;

UPDATE workspaces SET sync_epoch = gen_random_uuid();
