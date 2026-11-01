-- What Almanac keeps: topics, what each has kept (points, reminders,
-- questions) and the talks it was kept from. All of it syncs.

-- A topic: what it's called, what it sticks to, and how its page opens.
CREATE TABLE topics (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name text NOT NULL,
    -- A sentence or two on what the topic sticks to. Almanac writes it from
    -- the name when it's left empty.
    sticks_to text NOT NULL DEFAULT '',
    -- A Lucide icon name, from the ones the app knows.
    icon text NOT NULL DEFAULT 'book',
    -- The sidebar group it sits in, such as "Hobby".
    group_name text NOT NULL DEFAULT '',
    -- How the page opens: 'cards', a card per theme, or 'document', a
    -- document that grows.
    shape text NOT NULL DEFAULT 'cards' CHECK (shape IN ('cards', 'document')),
    -- For a document, its title.
    title text,
    -- First questions, for the empty page.
    starters jsonb NOT NULL DEFAULT '[]',
    position int NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX topics_workspace_idx ON topics (workspace_id, position);

-- A talk: a conversation with a topic, in the panel beside its page.
CREATE TABLE talks (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    topic_id uuid NOT NULL REFERENCES topics (id) ON DELETE CASCADE,
    -- The first question, short.
    title text NOT NULL,
    last_at timestamptz NOT NULL DEFAULT now(),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX talks_topic_idx ON talks (topic_id, last_at DESC);

-- What was said in a talk: what you asked, and Almanac's answers.
CREATE TABLE messages (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    talk_id uuid NOT NULL REFERENCES talks (id) ON DELETE CASCADE,
    topic_id uuid NOT NULL REFERENCES topics (id) ON DELETE CASCADE,
    role text NOT NULL CHECK (role IN ('you', 'almanac')),
    text text NOT NULL,
    -- Where an answer's points come from: [{ "title": ..., "url": ... }].
    sources jsonb NOT NULL DEFAULT '[]',
    -- What the answer kept, in words: "Kept on the page under Mieten."
    kept text,
    -- Why an answer stopped, when it didn't finish.
    problem text,
    -- The topic an off-topic question fits better, by name.
    elsewhere text,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX messages_talk_idx ON messages (talk_id, created_at);

-- A point the page keeps: short, under a theme, with where it comes from.
CREATE TABLE points (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    topic_id uuid NOT NULL REFERENCES topics (id) ON DELETE CASCADE,
    -- The card or section it's under: a plant, a part of a document.
    theme text NOT NULL,
    text text NOT NULL,
    -- Where it's from, in words: "Engels, Zur Wohnungsfrage, 1872".
    source text,
    source_url text,
    -- The talk it was kept from.
    talk_id uuid REFERENCES talks (id) ON DELETE SET NULL,
    -- Read once someone has seen it; new points carry the unread dot.
    seen boolean NOT NULL DEFAULT false,
    position int NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX points_topic_idx ON points (topic_id, theme, position);

-- A date in a topic: watering, a wash day, a reading circle. Those that
-- notify reach the Mac when they're due.
CREATE TABLE reminders (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    topic_id uuid NOT NULL REFERENCES topics (id) ON DELETE CASCADE,
    -- The card it belongs to, when it's about one: "Monstera".
    theme text,
    title text NOT NULL,
    -- When it's next due, and at what time of day (none: the whole day).
    due_on date NOT NULL,
    at time,
    -- Comes back this many days after it's done; none for once.
    every_days int CHECK (every_days IS NULL OR every_days BETWEEN 1 AND 366),
    -- Notifies the Mac when due. The rest show only in Almanac.
    notify boolean NOT NULL DEFAULT true,
    -- 'offered' waits for a yes in the talk; 'on' is on the calendar;
    -- 'done' was done once; 'declined' was turned down.
    status text NOT NULL DEFAULT 'on' CHECK (status IN ('offered', 'on', 'done', 'declined')),
    -- The answer that offered it.
    message_id uuid REFERENCES messages (id) ON DELETE SET NULL,
    last_done_on date,
    -- The due date the Mac was last told about, so it's told once each
    -- time the reminder comes due. Set by the server.
    notified_for date,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX reminders_due_idx ON reminders (workspace_id, due_on) WHERE status = 'on';

-- Something Almanac wasn't sure about. It waits where it's about.
CREATE TABLE questions (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    topic_id uuid NOT NULL REFERENCES topics (id) ON DELETE CASCADE,
    theme text,
    text text NOT NULL,
    -- The point it would add, when it's about keeping one.
    point text,
    source text,
    talk_id uuid REFERENCES talks (id) ON DELETE SET NULL,
    status text NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'added', 'left_out', 'answered')),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX questions_topic_idx ON questions (topic_id) WHERE status = 'open';

-- The Claude API key the desktop app's own server answers with, set in the
-- app's settings. Never synced. A server takes its key from its settings.
CREATE TABLE claude_key (
    only_one boolean PRIMARY KEY DEFAULT true CHECK (only_one),
    api_key text NOT NULL,
    updated_at timestamptz NOT NULL DEFAULT now()
);
