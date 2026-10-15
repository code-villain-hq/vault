-- A spec can belong to several services: the code it describes is often in
-- more than one place ("Signing in with GitHub" is both Accounts and the
-- GitHub connection). Services link a spec to code; areas to browse by come
-- later as domains.
ALTER TABLE specs ADD COLUMN service_ids uuid[] NOT NULL DEFAULT '{}';
UPDATE specs SET service_ids = ARRAY[service_id] WHERE service_id IS NOT NULL;
ALTER TABLE specs DROP COLUMN service_id;
CREATE INDEX specs_service_ids ON specs USING gin (service_ids);

UPDATE workspaces SET sync_epoch = gen_random_uuid();
