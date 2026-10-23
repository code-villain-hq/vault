-- Group resources arrive separately. Give a role transfer time to finish
-- before removing assignments and ending the person's sessions.
CREATE TABLE scim_departures (
    workspace_id uuid NOT NULL,
    user_id uuid NOT NULL,
    remove_after timestamptz NOT NULL DEFAULT now() + interval '30 seconds',
    PRIMARY KEY (workspace_id, user_id),
    FOREIGN KEY (workspace_id, user_id) REFERENCES workspace_members(workspace_id, user_id) ON DELETE CASCADE
);
