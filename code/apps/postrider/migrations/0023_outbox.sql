-- The outbox (`outbox.rs`): mail you send and drafts you keep, until their
-- servers have them. A mail to send keeps what goes over SMTP (`raw`, to
-- `envelope_to`) until it went, then what goes in Sent (`kept`, with its
-- Bcc) until it's there; a draft keeps only `kept`, for Drafts. Each shows
-- in its folder meanwhile under `placeholder`, a UID below zero. What it
-- replaces (the draft it was written from, by Message-ID) is deleted on
-- the server, and the mail it answers marked answered, once it's done.
CREATE TABLE outbox (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL REFERENCES mail_accounts(id) ON DELETE CASCADE,
    kind text NOT NULL CHECK (kind IN ('send', 'draft')),
    message_id text NOT NULL,
    envelope_from text,
    envelope_to text[] NOT NULL DEFAULT '{}',
    raw bytea,
    kept bytea,
    shown_path text,
    placeholder bigint UNIQUE,
    replaces_account uuid REFERENCES mail_accounts(id) ON DELETE SET NULL,
    replaces_message_id text,
    answered_account uuid REFERENCES mail_accounts(id) ON DELETE SET NULL,
    answered_path text,
    answered_uid bigint,
    at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX outbox_account ON outbox (account_id, at);
