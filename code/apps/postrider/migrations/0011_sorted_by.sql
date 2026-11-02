-- What made a newsletter or a notice when each account's mail was last
-- sorted (`newsletters.rs`): the newsletters' rule as Sieve says it. When
-- that changes, the account's Newsletters and Inbox are sorted again, once.
-- It takes the place of a date for each change.
ALTER TABLE newsletters_set_aside
    ADD COLUMN sorted_by text,
    DROP COLUMN notices_back_at,
    DROP COLUMN bulk_aside_at,
    DROP COLUMN services_aside_at;
