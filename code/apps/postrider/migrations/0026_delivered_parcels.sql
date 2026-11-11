-- Parcels you said came, by their number, or by their shop and order
-- while they have none. Parcels shows them delivered from then on, until
-- a mail says so itself.
CREATE TABLE delivered_parcels (
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    key text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, key)
);
