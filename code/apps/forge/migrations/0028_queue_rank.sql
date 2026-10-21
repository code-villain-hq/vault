-- The agents' work queue (FOR-68): the PO orders the issues agents take up
-- next. Issues with a rank come first, lowest first; the rest follow by
-- priority, then age. Null until the PO places an issue.
ALTER TABLE issues ADD COLUMN queue_rank double precision;

UPDATE workspaces SET sync_epoch = gen_random_uuid();
