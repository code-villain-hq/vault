-- Who answers on this Mac: Claude Code, or ChatGPT through Codex.
ALTER TABLE claude_choices
    ADD COLUMN answers text NOT NULL DEFAULT 'claude_code'
        CHECK (answers IN ('claude_code', 'chatgpt'));
