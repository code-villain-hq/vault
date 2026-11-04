-- What this Mac keeps of every account's folders but the Inbox
-- (`kept.rs`), so the sidebar and each folder show at once, and offline:
-- the folders as their server listed them last, in its order, with what
-- STATUS counted in each. `stamp` is what STATUS said when the folder's
-- lines were read last (UIDVALIDITY, UIDNEXT, MESSAGES and UNSEEN): null
-- until they are, and read again once it says something else.
CREATE TABLE kept_folders (
    account_id uuid NOT NULL REFERENCES mail_accounts(id) ON DELETE CASCADE,
    path text NOT NULL,
    name text NOT NULL,
    role text,
    position integer NOT NULL,
    total bigint NOT NULL DEFAULT 0,
    unread bigint NOT NULL DEFAULT 0,
    uid_validity bigint NOT NULL DEFAULT 0,
    stamp text,
    PRIMARY KEY (account_id, path)
);

-- Each kept folder's newest mail, as its list shows it, under the
-- folder's `uid_validity`. A folder renamed takes them along.
CREATE TABLE folder_mails (
    account_id uuid NOT NULL,
    path text NOT NULL,
    uid bigint NOT NULL,
    message_id text NOT NULL,
    from_name text NOT NULL,
    from_address text NOT NULL,
    to_name text NOT NULL,
    subject text NOT NULL,
    snippet text NOT NULL,
    date timestamptz NOT NULL,
    seen boolean NOT NULL,
    PRIMARY KEY (account_id, path, uid),
    FOREIGN KEY (account_id, path) REFERENCES kept_folders (account_id, path)
        ON DELETE CASCADE ON UPDATE CASCADE
);
CREATE INDEX folder_mails_newest ON folder_mails (account_id, path, date DESC, uid DESC);

-- A kept mail's source, read ahead or once it was opened, so it opens at
-- once and offline. It goes when its line does.
CREATE TABLE folder_sources (
    account_id uuid NOT NULL,
    path text NOT NULL,
    uid bigint NOT NULL,
    raw bytea NOT NULL,
    PRIMARY KEY (account_id, path, uid),
    FOREIGN KEY (account_id, path, uid) REFERENCES folder_mails (account_id, path, uid)
        ON DELETE CASCADE ON UPDATE CASCADE
);
