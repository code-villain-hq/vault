-- Accepting an issue out of Triage wakes the agent it's assigned to (FOR-25):
-- an agent that asked for a decision continues once the PO accepts it to Todo.
-- (0020 is taken by FOR-24's branch.)
ALTER TABLE agent_wakeups DROP CONSTRAINT agent_wakeups_event_check;
ALTER TABLE agent_wakeups ADD CONSTRAINT agent_wakeups_event_check
    CHECK (event IN ('assigned', 'comment', 'sent_back', 'mention', 'accepted'));
