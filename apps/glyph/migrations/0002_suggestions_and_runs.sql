-- A translation run: the server asking a model to translate a language's
-- missing keys. It either proposes them for review (suggest) or writes them
-- straight in (fill). Only the server writes runs; clients follow them.
CREATE TABLE translation_runs (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    kind text NOT NULL CHECK (kind IN ('suggest', 'fill')),
    source_lang text NOT NULL,
    target_lang text NOT NULL CHECK (target_lang <> source_lang),
    -- Only keys in this namespace; NULL for every namespace.
    namespace text,
    status text NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'running', 'done', 'failed', 'cancelled')),
    -- Keys to translate, keys translated, and keys the model failed on.
    total integer NOT NULL DEFAULT 0 CHECK (total >= 0),
    done integer NOT NULL DEFAULT 0 CHECK (done >= 0),
    failed integer NOT NULL DEFAULT 0 CHECK (failed >= 0),
    -- Why the run stopped, when it failed.
    error text NOT NULL DEFAULT '',
    -- The model's name from the server's models file.
    model text NOT NULL DEFAULT '',
    started_by uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    FOREIGN KEY (workspace_id, source_lang) REFERENCES languages (workspace_id, code),
    FOREIGN KEY (workspace_id, target_lang) REFERENCES languages (workspace_id, code)
);
CREATE INDEX translation_runs_workspace_idx ON translation_runs (workspace_id, created_at);

-- A model's proposed text for one key, waiting for someone to accept or
-- reject it. Accepting writes the translation and deletes the suggestion.
CREATE TABLE suggestions (
    id uuid PRIMARY KEY,
    workspace_id uuid NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    -- The run that made it; not a foreign key, so runs can go first.
    run_id uuid,
    lang text NOT NULL,
    namespace text NOT NULL,
    key text NOT NULL,
    value text NOT NULL,
    confidence double precision NOT NULL CHECK (confidence BETWEEN 0 AND 1),
    model text NOT NULL,
    -- The source text it was made from, to show what changed since.
    source_text text NOT NULL DEFAULT '',
    -- Glossary terms the model left out: [{source_term, target_term}].
    glossary_violations jsonb NOT NULL DEFAULT '[]',
    -- Whether {{ }} placeholders from the source went missing.
    placeholders_broken boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (workspace_id, lang, namespace, key),
    FOREIGN KEY (workspace_id, lang) REFERENCES languages (workspace_id, code)
);
