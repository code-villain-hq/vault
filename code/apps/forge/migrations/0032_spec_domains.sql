-- A spec's domain: the area of the product it's about, such as "Accounts",
-- the way OpenSpec keeps specs in domain folders. One per spec, or none; the
-- domains are the names in use, so there is nothing else to keep.
ALTER TABLE specs ADD COLUMN domain text
    CHECK (domain IS NULL OR (length(btrim(domain)) > 0 AND length(domain) <= 100));

UPDATE workspaces SET sync_epoch = gen_random_uuid();
