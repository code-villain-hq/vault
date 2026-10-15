-- Forge applies an issue's spec change when the issue is done (FOR-74), and
-- records that on it: when, the specs it changed, and the ones that no
-- longer fit it. A spec's history is the spec changes applied to it.
ALTER TABLE spec_changes
    ADD COLUMN applied_at timestamptz,
    ADD COLUMN applied_spec_ids uuid[] NOT NULL DEFAULT '{}',
    -- [{spec_id, title, reason}]
    ADD COLUMN not_applied jsonb NOT NULL DEFAULT '[]';

-- Spec changes of issues already done were applied by hand.
UPDATE spec_changes c
SET applied_at = c.updated_at,
    applied_spec_ids = ARRAY(
        SELECT (d ->> 'spec_id')::uuid FROM jsonb_array_elements(c.specs) AS d
        WHERE d ->> 'spec_id' IS NOT NULL
    )
FROM issues i
JOIN workflow_states s ON s.id = i.state_id
WHERE i.id = c.issue_id AND s.type = 'completed';

UPDATE workspaces SET sync_epoch = gen_random_uuid();
