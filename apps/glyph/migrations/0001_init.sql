-- Glyph's schema. The sync engine's tables (workspaces, users, sync_log,
-- sync_transactions) and the accounts (members, sessions, invites) are the
-- same as Forge's; the synced models after them are Glyph's own.

CREATE TABLE workspaces (
    id uuid PRIMARY KEY,
    name text NOT NULL,
    -- The workspace's name in URLs, e.g. /lair/translations.
    url_key text NOT NULL UNIQUE,
    -- Changes whenever the workspace's sync history starts over (for example
    -- after a database reset), so clients know to download a fresh snapshot.
    sync_epoch uuid NOT NULL DEFAULT gen_random_uuid(),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE users (
    id uuid PRIMARY KEY,
    email text NOT NULL UNIQUE,
    name text NOT NULL,
    password_hash text,
    created_at timestamptz NOT NULL DEFAULT now()
);

-- Every change to a synced record, in order. Clients download a snapshot once
-- and then follow this log. Within a workspace, sync ids are assigned in
-- commit order because writes to a workspace are serialised.
CREATE TABLE sync_log (
    sync_id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    transaction_id uuid NOT NULL,
    actor_id uuid NOT NULL REFERENCES users (id),
    model text NOT NULL,
    model_id uuid NOT NULL,
    action text NOT NULL CHECK (action IN ('insert', 'update', 'delete')),
    -- The whole record after the change; NULL for deletes.
    data jsonb,
    -- For a record only one person syncs, that person; NULL for everyone.
    visible_to uuid,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX sync_log_workspace_idx ON sync_log (workspace_id, sync_id);
CREATE INDEX sync_log_model_idx ON sync_log (model_id, sync_id);
CREATE INDEX sync_log_transaction_idx ON sync_log (transaction_id);

-- Transactions that have been applied, so a client that retries one (because
-- it never saw the response) doesn't apply it twice.
CREATE TABLE sync_transactions (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    actor_id uuid NOT NULL REFERENCES users (id),
    -- The last sync id the transaction produced; NULL if it changed nothing.
    last_sync_id bigint,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE workspace_members (
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role text NOT NULL CHECK (role IN ('admin', 'member')),
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, user_id)
);
CREATE INDEX workspace_members_user_idx ON workspace_members (user_id);

-- Only a hash of each session token is stored, so a leaked database can't be
-- used to sign in.
CREATE TABLE sessions (
    token_hash bytea PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL
);
CREATE INDEX sessions_user_idx ON sessions (user_id);

-- An invite link can be used by several people until it expires or an admin
-- revokes it.
CREATE TABLE invites (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    token_hash bytea NOT NULL UNIQUE,
    role text NOT NULL CHECK (role IN ('admin', 'member')),
    created_by uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    revoked_at timestamptz
);
CREATE INDEX invites_workspace_idx ON invites (workspace_id);

-- The people in each workspace, as clients sync them. Read-only: membership
-- changes go through the account API, which logs them.
CREATE VIEW workspace_users AS
SELECT m.user_id AS id, m.workspace_id, u.name, u.email, m.role, m.created_at
FROM workspace_members m
JOIN users u ON u.id = m.user_id;

-- The languages a workspace translates between. One of them is the source:
-- the language keys are written in first.
CREATE TABLE languages (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    code text NOT NULL,
    is_source boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, code)
);
CREATE UNIQUE INDEX languages_one_source_idx ON languages (workspace_id) WHERE is_source;

-- One text of one key in one language, with where it came from. A key lives
-- in a namespace, as in i18next and locize.
CREATE TABLE translations (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    lang text NOT NULL,
    namespace text NOT NULL CHECK (length(btrim(namespace)) > 0),
    key text NOT NULL CHECK (length(btrim(key)) > 0),
    value text NOT NULL,
    -- How sure the model was, from 0 to 1; NULL when a person wrote it.
    confidence double precision CHECK (confidence BETWEEN 0 AND 1),
    -- "human", or the name of the model that translated it.
    translated_by text NOT NULL DEFAULT 'human',
    -- A note for translators about where the text appears.
    context text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, lang, namespace, key),
    FOREIGN KEY (workspace_id, lang) REFERENCES languages (workspace_id, code)
);
CREATE INDEX translations_key_idx ON translations (workspace_id, namespace, key);

-- Preferred terms: "in Swedish, always translate email as e-post".
CREATE TABLE glossary_entries (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    source_lang text NOT NULL,
    target_lang text NOT NULL,
    source_term text NOT NULL CHECK (length(btrim(source_term)) > 0),
    target_term text NOT NULL CHECK (length(btrim(target_term)) > 0),
    -- Other accepted forms of the target term, such as inflections.
    target_variants text[] NOT NULL DEFAULT '{}',
    -- For reviewers only; never sent to the model.
    note text NOT NULL DEFAULT '',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (workspace_id, source_lang) REFERENCES languages (workspace_id, code),
    FOREIGN KEY (workspace_id, target_lang) REFERENCES languages (workspace_id, code)
);
CREATE UNIQUE INDEX glossary_entries_term_idx
    ON glossary_entries (workspace_id, source_lang, target_lang, lower(btrim(source_term)));

-- How to translate in one direction: which model, how freely, and below
-- which confidence a translation needs a look.
CREATE TABLE language_pairs (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    source_lang text NOT NULL,
    target_lang text NOT NULL CHECK (target_lang <> source_lang),
    -- A model's name from the server's models file; empty for the first one.
    model text NOT NULL DEFAULT '',
    temperature double precision NOT NULL DEFAULT 0.2 CHECK (temperature BETWEEN 0 AND 2),
    confidence_threshold double precision NOT NULL DEFAULT 0.7
        CHECK (confidence_threshold BETWEEN 0 AND 1),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, source_lang, target_lang),
    FOREIGN KEY (workspace_id, source_lang) REFERENCES languages (workspace_id, code),
    FOREIGN KEY (workspace_id, target_lang) REFERENCES languages (workspace_id, code)
);
