-- Notices that say where to unsubscribe (a sign-in, an order, a delivery)
-- aren't newsletters (`newsletters.rs`). An account set aside before they
-- were told apart gets its rule written again and those notices moved back
-- to the Inbox, once: this says when.
ALTER TABLE newsletters_set_aside ADD COLUMN notices_back_at timestamptz;
