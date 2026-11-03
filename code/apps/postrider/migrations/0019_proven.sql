-- Whether the mail Claude Code read for a verdict was signed by its
-- sender's own organisation (DKIM, `proof.rs`). Only those name a sender in
-- the newsletters' rule (`judging.rs`): anyone can write a bank's address
-- in From. Senders named so far come off the rule until their own signed
-- mail is read.
ALTER TABLE newsletter_verdicts ADD COLUMN proven boolean NOT NULL DEFAULT false;
