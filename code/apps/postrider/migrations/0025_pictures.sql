-- Senders whose mail shows its pictures, and whatever else it loads from
-- the web, as it opens. Everyone else's waits for "Load pictures", so a
-- sender can't see that, or when, it was read.
CREATE TABLE picture_senders (
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    address text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, address)
);
