-- Agents propose a new description for an issue instead of changing its
-- scope in a comment; people accept or dismiss it on the issue.
ALTER TABLE suggestions
    DROP CONSTRAINT suggestions_kind_check,
    ADD CONSTRAINT suggestions_kind_check CHECK (kind IN ('drift', 'triage', 'description'));
