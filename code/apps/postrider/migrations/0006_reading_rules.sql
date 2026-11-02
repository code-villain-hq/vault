-- Sorting that needs reading a mail ("anything that looks like an invoice"),
-- which no rule on the mail server can do: Postrider reads new mail in the
-- Inbox on this Mac, while it's open, and moves what fits (`reading.rs`).
CREATE TABLE reading_rules (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL REFERENCES mail_accounts (id) ON DELETE CASCADE,
    -- The folder as Postrider shows it, and its path on the server.
    folder text NOT NULL,
    path text NOT NULL,
    -- What its mail looks like, finishing "mail that looks like …".
    about text NOT NULL,
    -- Mail that came to the Inbox before this isn't read for it.
    created_at timestamptz NOT NULL
);

CREATE INDEX reading_rules_account ON reading_rules (account_id);

-- The mail's Message-ID, so a mail moved back to the Inbox, under a new
-- UID, is known again.
ALTER TABLE inbox_mails ADD COLUMN message_id text NOT NULL DEFAULT '';

-- The mail the sorter has read, by Message-ID (else `uid <n>`): each is read
-- once, so one you move back to the Inbox stays there.
CREATE TABLE reading_read (
    account_id uuid NOT NULL REFERENCES mail_accounts (id) ON DELETE CASCADE,
    message_id text NOT NULL,
    read_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (account_id, message_id)
);
