-- A topic is its blocks, in order, and has no shape: each block lays out
-- its points one way (today's steps, the week, chips, a shelf, cards, text
-- or a list) under a title of the topic's own. A template only names the
-- blocks a topic starts with.
CREATE TABLE blocks (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    topic_id uuid NOT NULL REFERENCES topics (id) ON DELETE CASCADE,
    kind text NOT NULL
        CHECK (kind IN ('today', 'week', 'chips', 'shelf', 'cards', 'text', 'list')),
    -- Empty for none: the week is headed by its days.
    title text NOT NULL DEFAULT '',
    position int NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX blocks_topic_idx ON blocks (topic_id, position);

-- The blocks each shape laid out. A routine's page had them all; cards and
-- documents had their week when something came round, and a document's
-- sections are what it says, under its own title when it had one.
INSERT INTO blocks (id, workspace_id, topic_id, kind, title, position)
SELECT gen_random_uuid(), t.workspace_id, t.id, b.kind, b.title, b.position
FROM topics t
CROSS JOIN LATERAL (VALUES
    ('today', 'Today', 0),
    ('week', '', 1),
    ('chips', 'You', 2),
    ('shelf', 'Your shelf', 3),
    ('list', 'Advice', 4)
) AS b (kind, title, position)
WHERE t.kind = 'topic' AND t.shape = 'routine';

INSERT INTO blocks (id, workspace_id, topic_id, kind, title, position)
SELECT gen_random_uuid(), t.workspace_id, t.id, 'week', '', 0
FROM topics t
WHERE t.kind = 'topic' AND t.shape <> 'routine'
  AND EXISTS (SELECT 1 FROM reminders r WHERE r.topic_id = t.id);

INSERT INTO blocks (id, workspace_id, topic_id, kind, title, position)
SELECT gen_random_uuid(), t.workspace_id, t.id,
       CASE t.shape WHEN 'document' THEN 'text' ELSE 'cards' END,
       CASE t.shape WHEN 'document'
           THEN COALESCE(NULLIF(NULLIF(t.title, ''), t.name), 'What it says') ELSE 'Notes' END,
       5
FROM topics t
WHERE t.kind = 'topic'
  AND (t.shape <> 'routine' OR EXISTS (
      SELECT 1 FROM points p WHERE p.topic_id = t.id AND p.kind = 'note'));

-- Each point goes in the block that showed it.
ALTER TABLE points ADD COLUMN block_id uuid REFERENCES blocks (id) ON DELETE CASCADE;
UPDATE points p SET block_id = b.id
FROM blocks b
WHERE b.topic_id = p.topic_id
  AND b.kind = CASE p.kind
      WHEN 'about' THEN 'chips'
      WHEN 'thing' THEN 'shelf'
      WHEN 'step' THEN 'today'
      WHEN 'tip' THEN 'list'
      ELSE (SELECT kind FROM blocks WHERE topic_id = p.topic_id AND position = 5 LIMIT 1)
  END;

-- Any left over, such as the own topic's, get a block of their own.
INSERT INTO blocks (id, workspace_id, topic_id, kind, title, position)
SELECT gen_random_uuid(), p.workspace_id, p.topic_id, 'cards', 'Notes', 6
FROM points p WHERE p.block_id IS NULL
GROUP BY p.workspace_id, p.topic_id;
UPDATE points p SET block_id = b.id
FROM blocks b
WHERE p.block_id IS NULL AND b.topic_id = p.topic_id AND b.position = 6;

ALTER TABLE points ALTER COLUMN block_id SET NOT NULL;
ALTER TABLE points ALTER COLUMN theme SET DEFAULT '';
CREATE INDEX points_block_idx ON points (block_id, position);
ALTER TABLE points DROP COLUMN kind;
ALTER TABLE topics DROP COLUMN shape;
ALTER TABLE topics DROP COLUMN title;

-- Answers kept what they proposed and changed with the old fields: a
-- proposal now names its template, and its points their block.
UPDATE messages SET proposal = (proposal - 'shape' - 'title')
    || jsonb_build_object('template', CASE proposal->>'shape'
        WHEN 'routine' THEN 'routine'
        WHEN 'document' THEN 'study'
        ELSE 'collection' END)
    || jsonb_build_object('points', COALESCE((
        SELECT jsonb_agg((p - 'kind') || CASE p->>'kind'
            WHEN 'about' THEN '{"block": "You"}'::jsonb
            WHEN 'thing' THEN '{"block": "Your shelf"}'::jsonb
            WHEN 'step' THEN '{"block": "Today"}'::jsonb
            WHEN 'tip' THEN '{"block": "Advice"}'::jsonb
            ELSE '{}'::jsonb END)
        FROM jsonb_array_elements(proposal->'points') p), '[]'::jsonb))
WHERE proposal IS NOT NULL;
UPDATE messages SET changes = (
    SELECT jsonb_agg(c || jsonb_build_object(
        'before', (c->'before') - 'shape' - 'title',
        'after', (c->'after') - 'shape' - 'title'))
    FROM jsonb_array_elements(changes) c)
WHERE changes <> '[]'::jsonb;

UPDATE workspaces SET sync_epoch = gen_random_uuid();
