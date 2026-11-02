-- Whose colour each calendar on the server wears, where the person chose:
-- its address there (its href) to the account's id. A calendar left out
-- wears the colour of the account it's named after, or at its domain.
ALTER TABLE calendars ADD COLUMN accounts jsonb NOT NULL DEFAULT '{}';

-- Clients take a fresh copy, with the new field.
UPDATE workspaces SET sync_epoch = gen_random_uuid();
