-- What was done to mail while its server couldn't be reached (`queued.rs`):
-- done on this Mac's copy at once, and on the server once it answers.
--
-- A mail to mark or move: where its server has it (`path` and `uid`, under
-- `uid_validity`), whether it's to be read (`seen`) and the folder it goes
-- to (`to_path`). One moved shows there under `placeholder`, a UID below
-- zero, until its server gives it its own. Its line as it showed where it
-- was is kept beside it, for when it moves back.
CREATE TABLE queued_mails (
    account_id uuid NOT NULL REFERENCES mail_accounts(id) ON DELETE CASCADE,
    path text NOT NULL,
    uid bigint NOT NULL,
    uid_validity bigint NOT NULL,
    seen boolean,
    to_path text,
    placeholder bigint UNIQUE,
    message_id text,
    from_name text,
    from_address text,
    to_name text,
    subject text,
    snippet text,
    date timestamptz,
    refs text[],
    to_addresses text[],
    reply_to text,
    at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (account_id, path, uid)
);
CREATE SEQUENCE queued_placeholders;

-- A folder marked read: every mail in it under UID `below` that's unread
-- (UIDs from `below` on came after).
CREATE TABLE queued_reads (
    id uuid PRIMARY KEY,
    account_id uuid NOT NULL REFERENCES mail_accounts(id) ON DELETE CASCADE,
    path text NOT NULL,
    uid_validity bigint NOT NULL,
    below bigint NOT NULL,
    at timestamptz NOT NULL DEFAULT now()
);
