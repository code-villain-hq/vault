-- What joins mail in conversations (`conversations.rs`): the Message-IDs a
-- mail answers (In-Reply-To and References), whom it went to (To and Cc)
-- and whom answers go to (Reply-To), addresses in lower case. An Inbox
-- mail copied before this has none yet (`refs` is null) until its header
-- is read again.
ALTER TABLE inbox_mails
    ADD COLUMN refs text[],
    ADD COLUMN to_addresses text[] NOT NULL DEFAULT '{}',
    ADD COLUMN reply_to text NOT NULL DEFAULT '';

-- What you sent and archived, from an account's Sent and Archive (by its
-- role's key), as far back as its Inbox's oldest mail and a month more:
-- what joins its Inbox's mail in conversations.
CREATE TABLE filed_mails (
    account_id uuid NOT NULL REFERENCES mail_accounts(id) ON DELETE CASCADE,
    folder text NOT NULL,
    uid_validity bigint NOT NULL,
    uid bigint NOT NULL,
    message_id text NOT NULL,
    refs text[] NOT NULL,
    from_name text NOT NULL,
    from_address text NOT NULL,
    to_name text NOT NULL,
    to_addresses text[] NOT NULL,
    reply_to text NOT NULL,
    subject text NOT NULL,
    snippet text NOT NULL,
    date timestamptz NOT NULL,
    seen boolean NOT NULL,
    PRIMARY KEY (account_id, folder, uid)
);
