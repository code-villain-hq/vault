-- The reminders an offer takes the place of: a wash day that moved. They
-- go when the offer is taken, and stay when it's turned down.
ALTER TABLE reminders ADD COLUMN replaces jsonb NOT NULL DEFAULT '[]';
