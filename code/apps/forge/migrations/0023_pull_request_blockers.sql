-- An issue counts as ready for review only once its pull request could be
-- merged (FOR-30). A pull request link records what stands in the way, if
-- anything: conflicts with the default branch, failing checks, or checks
-- still running. Forge asks GitHub again whenever the default branch moves
-- or checks finish.
ALTER TABLE issue_links ADD COLUMN blocker text
    CHECK (blocker IN ('conflicts', 'checks_failed', 'checks_running'));
-- Forge is holding the issue back from review because of the blocker, and
-- moves it to review once the blocker is gone.
ALTER TABLE issue_links ADD COLUMN held boolean NOT NULL DEFAULT false;

-- A pull request that runs into conflicts or failing checks wakes the agent
-- working on the issue.
ALTER TABLE agent_wakeups DROP CONSTRAINT agent_wakeups_event_check;
ALTER TABLE agent_wakeups ADD CONSTRAINT agent_wakeups_event_check
    CHECK (event IN ('assigned', 'comment', 'sent_back', 'mention', 'accepted', 'pr_blocked'));

UPDATE workspaces SET sync_epoch = gen_random_uuid();
