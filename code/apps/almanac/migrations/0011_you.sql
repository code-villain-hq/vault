-- What a topic knows about them is a block of its own, `you`, which the
-- topic's About shows and its page doesn't. A routine's You chips become
-- it, and every other topic gets one, empty, after its other blocks.
ALTER TABLE blocks DROP CONSTRAINT blocks_kind_check;
ALTER TABLE blocks ADD CONSTRAINT blocks_kind_check
    CHECK (kind IN ('today', 'week', 'chips', 'shelf', 'cards', 'text', 'list', 'timeline', 'you'));

UPDATE blocks SET kind = 'you', updated_at = now()
WHERE kind = 'chips' AND lower(title) IN ('you', 'du');

INSERT INTO blocks (id, workspace_id, topic_id, kind, title, position)
SELECT gen_random_uuid(), t.workspace_id, t.id, 'you', 'You',
    coalesce((SELECT max(b.position) + 1 FROM blocks b WHERE b.topic_id = t.id), 0)
FROM topics t
WHERE t.kind = 'topic'
    AND NOT EXISTS (SELECT 1 FROM blocks b WHERE b.topic_id = t.id AND b.kind = 'you');

UPDATE workspaces SET sync_epoch = gen_random_uuid();
