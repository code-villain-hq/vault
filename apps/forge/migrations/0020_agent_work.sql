-- What an agent is working on (FOR-24): the agent says when it starts and
-- finishes an issue, and any call it makes shows it's still at it. Clients
-- learn it from the agent's user record.

ALTER TABLE workspace_members
    ADD COLUMN working_on_id uuid REFERENCES issues (id) ON DELETE SET NULL,
    ADD COLUMN working_since timestamptz,
    -- The agent's latest sign of life while working. An hour without one
    -- means the run stopped.
    ADD COLUMN working_seen_at timestamptz;

CREATE OR REPLACE VIEW workspace_users AS
SELECT m.user_id AS id, m.workspace_id, u.name, u.email, m.role, m.created_at, u.kind,
       m.working_on_id, m.working_since, m.working_seen_at
FROM workspace_members m
JOIN users u ON u.id = m.user_id;

-- User records gained fields outside the sync log.
UPDATE workspaces SET sync_epoch = gen_random_uuid();
