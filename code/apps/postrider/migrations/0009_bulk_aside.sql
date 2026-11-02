-- Newsletters may say a machine sent them (`Auto-Submitted`) to keep
-- auto-replies away; when they also say they went to many, they're
-- newsletters still (`newsletters.rs`). An account whose notices were moved
-- back before gets its rule written again and those newsletters set aside,
-- once: this says when.
ALTER TABLE newsletters_set_aside ADD COLUMN bulk_aside_at timestamptz;
