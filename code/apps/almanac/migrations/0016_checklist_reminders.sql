-- A checklist item that repeats and rings: its reminder names it, and
-- ticking the item on the page ticks the reminder. No foreign key: the
-- sync engine doesn't hear of what Postgres would set to null, so an item
-- that's gone leaves a reminder that names nothing, which Calendar shows
-- as it shows the rest.
ALTER TABLE reminders ADD COLUMN point_id uuid;

UPDATE workspaces SET sync_epoch = gen_random_uuid();
