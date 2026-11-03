-- Claude Code's question changed (`judging.rs`): what a service has to tell
-- every customer (new terms, a privacy policy, how to delete your account, a
-- security notice) is yours, not sent to many. What it found sent to many is
-- asked again, so Newsletters is read once more and the notices go back.
DELETE FROM newsletter_verdicts WHERE many;
UPDATE newsletter_judged SET read = false;
UPDATE newsletters_set_aside SET judge_again = true;
