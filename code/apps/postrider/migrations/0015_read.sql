-- Whether Claude Code read a judged mail, not only looked at it: what its
-- header said went to many used to go to Newsletters judged but unread, so
-- Newsletters reads once more what it didn't (`judge_again`).
ALTER TABLE newsletter_judged ADD COLUMN read boolean NOT NULL DEFAULT false;

UPDATE newsletters_set_aside SET judge_again = true;
