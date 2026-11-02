-- Where each account's Newsletters stood when its page was last read
-- (`newsletters.rs`): its UIDVALIDITY and UIDNEXT. The unread mail that
-- came after is the number beside Newsletters in the sidebar.
ALTER TABLE newsletters_set_aside
    ADD COLUMN looked_validity bigint,
    ADD COLUMN looked_uid bigint;
