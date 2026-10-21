-- The Queue view, the only way to set an issue's place in the agents' queue,
-- was removed; agents take their issues by priority, then age.
ALTER TABLE issues DROP COLUMN queue_rank;
UPDATE workspaces SET sync_epoch = gen_random_uuid();
