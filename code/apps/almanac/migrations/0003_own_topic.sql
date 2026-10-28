-- Every workspace has one topic of its own, called Almanac: it knows the
-- other topics, proposes new ones and changes them when asked. It can be
-- renamed, not deleted.
ALTER TABLE topics ADD COLUMN kind text NOT NULL DEFAULT 'topic'
    CHECK (kind IN ('topic', 'almanac'));
CREATE UNIQUE INDEX topics_own_idx ON topics (workspace_id) WHERE kind = 'almanac';

-- A topic Almanac proposes in an answer, with the topic's id once it was
-- started.
ALTER TABLE messages ADD COLUMN proposal jsonb;
-- What an answer changed in topics, with what was there before, for Undo.
ALTER TABLE messages ADD COLUMN changes jsonb NOT NULL DEFAULT '[]';

-- How Claude Code on this Mac answers: whether it may search the web.
-- Never synced.
CREATE TABLE claude_choices (
    only_one boolean PRIMARY KEY DEFAULT true CHECK (only_one),
    web boolean NOT NULL DEFAULT true,
    updated_at timestamptz NOT NULL DEFAULT now()
);
