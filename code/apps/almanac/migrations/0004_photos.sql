-- The photos a question came with. A message names its photos (id and
-- size); the bytes stay here and don't sync, and go with the message.
ALTER TABLE messages ADD COLUMN photos jsonb NOT NULL DEFAULT '[]';

CREATE TABLE photos (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    message_id uuid NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
    -- Its place among the question's photos.
    position int NOT NULL,
    media_type text NOT NULL,
    data bytea NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX photos_message_idx ON photos (message_id);
