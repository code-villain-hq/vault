-- Pictures beyond a question's photos: a topic's cover, and a row's photo
-- dropped on the page or fetched from the web by a talk. Those belong to no
-- message, and stay until the workspace goes.
ALTER TABLE photos ALTER COLUMN message_id DROP NOT NULL;

-- The photo a topic's page opens with, and its tile shows. No foreign key,
-- as with a row's photo cell.
ALTER TABLE topics ADD COLUMN cover uuid;

UPDATE workspaces SET sync_epoch = gen_random_uuid();
