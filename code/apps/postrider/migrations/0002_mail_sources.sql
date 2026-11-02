-- A mail's source, kept once it has been opened, so it opens again at once
-- and offline. It goes when its line leaves the Inbox's copy.
CREATE TABLE mail_sources (
    account_id uuid NOT NULL,
    uid bigint NOT NULL,
    raw bytea NOT NULL,
    PRIMARY KEY (account_id, uid),
    FOREIGN KEY (account_id, uid) REFERENCES inbox_mails (account_id, uid) ON DELETE CASCADE
);
