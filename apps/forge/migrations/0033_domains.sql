-- Domains become records: the areas of the product specs are about, with a
-- name, what the area covers, and an icon and colour like services, so they
-- can be renamed, described and removed in one place. A spec points to its
-- domain; the names specs had become domains.
CREATE TABLE domains (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    name text NOT NULL CHECK (length(btrim(name)) > 0),
    -- Markdown: what the area covers.
    description text NOT NULL DEFAULT '',
    icon text NOT NULL DEFAULT 'folder',
    color text NOT NULL DEFAULT '#8a8a94',
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (id, workspace_id)
);

INSERT INTO domains (id, workspace_id, name, color)
SELECT uuidv7(), workspace_id, domain,
       (ARRAY['#5b5bd6', '#4cb782', '#f2c94c', '#f2994a', '#d1453b', '#bb87fc', '#4ea7fc', '#8a8a94'])
           [((row_number() OVER (PARTITION BY workspace_id ORDER BY domain) - 1) % 8) + 1]
FROM (SELECT DISTINCT workspace_id, domain FROM specs WHERE domain IS NOT NULL) AS named;

ALTER TABLE specs ADD COLUMN domain_id uuid;
ALTER TABLE specs ADD FOREIGN KEY (domain_id, workspace_id)
    REFERENCES domains (id, workspace_id) ON DELETE SET NULL (domain_id);
UPDATE specs SET domain_id = d.id
FROM domains d
WHERE d.workspace_id = specs.workspace_id AND d.name = specs.domain;
ALTER TABLE specs DROP COLUMN domain;

UPDATE workspaces SET sync_epoch = gen_random_uuid();
