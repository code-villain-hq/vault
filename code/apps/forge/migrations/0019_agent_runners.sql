-- Waking agents (FOR-13): an agent's owner can register a runner, and Forge
-- POSTs a signed wake-up to it when there's work for the agent. Forge only
-- notifies; the runner decides what to do.

-- At most one runner per agent.
CREATE TABLE agent_runners (
    workspace_id uuid NOT NULL,
    user_id uuid NOT NULL,
    url text NOT NULL,
    -- Signs each wake-up so the runner can tell it came from Forge. Kept as
    -- is (signing needs it), and shown only when it's made.
    secret text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, user_id),
    FOREIGN KEY (workspace_id, user_id) REFERENCES workspace_members (workspace_id, user_id) ON DELETE CASCADE
);

-- Wake-ups to deliver, and the record of those delivered or given up on.
CREATE TABLE agent_wakeups (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL,
    -- The agent.
    user_id uuid NOT NULL,
    event text NOT NULL CHECK (event IN ('assigned', 'comment', 'sent_back', 'mention')),
    issue_id uuid NOT NULL,
    status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'delivered', 'failed')),
    attempts integer NOT NULL DEFAULT 0,
    next_attempt_at timestamptz NOT NULL DEFAULT now(),
    -- Why the last attempt failed: "HTTP 500", "timed out", ...
    last_error text,
    created_at timestamptz NOT NULL DEFAULT now(),
    delivered_at timestamptz,
    FOREIGN KEY (workspace_id, user_id) REFERENCES workspace_members (workspace_id, user_id) ON DELETE CASCADE,
    FOREIGN KEY (issue_id, workspace_id) REFERENCES issues (id, workspace_id) ON DELETE CASCADE
);
CREATE INDEX agent_wakeups_due_idx ON agent_wakeups (next_attempt_at) WHERE status = 'pending';
CREATE INDEX agent_wakeups_agent_idx ON agent_wakeups (workspace_id, user_id, created_at DESC);
