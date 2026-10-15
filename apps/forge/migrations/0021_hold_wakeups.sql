-- Holding wake-ups while an agent is busy (FOR-26): Forge sends one at a
-- time, and waits for the agent to take it up before sending the next.

-- When Forge last woke the agent, until it starts work. Another wake-up
-- waits until then, or for a while if it never does.
ALTER TABLE agent_runners ADD COLUMN woken_at timestamptz;
