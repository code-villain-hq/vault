-- The accounts whose newsletters Postrider has set aside once
-- (`newsletters.rs`): their rule written on the mail server and the Inbox's
-- newsletters moved. A rule you take away later doesn't come back.
CREATE TABLE newsletters_set_aside (
    account_id uuid PRIMARY KEY REFERENCES mail_accounts (id) ON DELETE CASCADE,
    set_aside_at timestamptz NOT NULL DEFAULT now()
);
