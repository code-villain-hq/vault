-- Postrider's schema: the sync engine's tables (code/kit/sync/schema.sql),
-- you and your workspace, the accounts it reads, and what it keeps of their
-- Inboxes. Mail stays on the mail servers: the Inbox here is a copy of the
-- newest headers, for showing the list at once and offline, and it can be
-- thrown away and read again.

CREATE TABLE users (
    id uuid PRIMARY KEY,
    email text NOT NULL UNIQUE,
    name text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE sessions (
    token_hash bytea PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX sessions_user ON sessions (user_id);

CREATE TABLE workspaces (
    id uuid PRIMARY KEY,
    name text NOT NULL,
    -- The workspace's name in URLs, e.g. /michael.
    url_key text NOT NULL UNIQUE,
    -- Changes whenever the workspace's sync history starts over (for example
    -- after a database reset), so clients know to download a fresh snapshot.
    -- Any migration that changes synced records directly must change it too.
    sync_epoch uuid NOT NULL DEFAULT gen_random_uuid(),
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
    role text NOT NULL CHECK (role IN ('admin', 'member', 'guest')),
    -- What a guest may see; Postrider has no guests yet.
    access_version bigint NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (workspace_id, user_id)
);
CREATE INDEX workspace_members_user_idx ON workspace_members (user_id);

-- The people in each workspace, as clients sync them. Read-only.
CREATE VIEW workspace_users AS
SELECT m.user_id AS id, m.workspace_id, u.name, u.email, m.role, m.created_at
FROM workspace_members m
JOIN users u ON u.id = m.user_id;

-- A mail account Postrider reads over IMAP. Its password is in the macOS
-- keychain, never in the database.
CREATE TABLE mail_accounts (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name text NOT NULL,
    address text NOT NULL,
    host text NOT NULL,
    port int NOT NULL,
    username text NOT NULL,
    -- The data hue, 1 to 6, its senders and its calendar wear.
    hue int NOT NULL CHECK (hue BETWEEN 1 AND 6),
    position int NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, address)
);

-- The calendar server, read over CalDAV: one per workspace. Its password
-- is in the keychain too.
CREATE TABLE calendars (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL UNIQUE REFERENCES workspaces (id) ON DELETE CASCADE,
    url text NOT NULL,
    username text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

-- The newest mail in each account's Inbox, as the list shows it. The IMAP
-- server's UIDVALIDITY says whether the UIDs still mean the same mails;
-- when it changes, the copy is read again.
CREATE TABLE inboxes (
    account_id uuid PRIMARY KEY REFERENCES mail_accounts (id) ON DELETE CASCADE,
    uid_validity bigint NOT NULL
);

CREATE TABLE inbox_mails (
    account_id uuid NOT NULL REFERENCES inboxes (account_id) ON DELETE CASCADE,
    uid bigint NOT NULL,
    from_name text NOT NULL,
    from_address text NOT NULL,
    subject text NOT NULL,
    snippet text NOT NULL,
    date timestamptz NOT NULL,
    seen boolean NOT NULL,
    PRIMARY KEY (account_id, uid)
);
CREATE INDEX inbox_mails_date_idx ON inbox_mails (date DESC);
