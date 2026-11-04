-- What each account signs the mail it sends with, below a "-- " line.
-- Empty: it signs nothing.
ALTER TABLE mail_accounts ADD COLUMN signature text NOT NULL DEFAULT '';

-- Clients take a fresh copy, with the new field.
UPDATE workspaces SET sync_epoch = gen_random_uuid();
