-- Some newsletters offer to unsubscribe only in their text, and are known
-- by the mailing service that sent them (`newsletters.rs`). An account set
-- aside before gets its rule written again and those newsletters set
-- aside, once: this says when.
ALTER TABLE newsletters_set_aside ADD COLUMN services_aside_at timestamptz;
