-- What Claude Code made of mail that offers to unsubscribe but doesn't say
-- it went to many (`judging.rs`): by sender and kind of mail (its subject,
-- numbers left out), whether it went to many, so the same isn't asked
-- twice.
CREATE TABLE newsletter_verdicts (
    account_id uuid NOT NULL REFERENCES mail_accounts (id) ON DELETE CASCADE,
    address text NOT NULL,
    kind text NOT NULL,
    many boolean NOT NULL,
    judged_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (account_id, address, kind)
);

-- The mails judged, by Message-ID, so one you move back stays where you
-- put it.
CREATE TABLE newsletter_judged (
    account_id uuid NOT NULL REFERENCES mail_accounts (id) ON DELETE CASCADE,
    message_id text NOT NULL,
    PRIMARY KEY (account_id, message_id)
);

-- Newsletters' mail that its header can't tell, to judge once after the
-- account's mail was sorted again.
ALTER TABLE newsletters_set_aside ADD COLUMN judge_again boolean NOT NULL DEFAULT false;
