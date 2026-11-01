-- Points gained their kind in 0006: clients download a fresh copy, so the
-- points they have carry it too.
UPDATE workspaces SET sync_epoch = gen_random_uuid();
