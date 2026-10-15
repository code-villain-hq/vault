-- Service icons now come from Tabler's outline icons; the ones picked so far
-- get their Tabler names.
UPDATE services SET icon = CASE icon
    WHEN 'window' THEN 'app-window'
    WHEN 'monitor' THEN 'device-desktop'
    WHEN 'sparkle' THEN 'sparkles'
    WHEN 'branch' THEN 'git-branch'
    WHEN 'sync' THEN 'refresh'
    WHEN 'bot' THEN 'robot'
    ELSE icon
END;

UPDATE workspaces SET sync_epoch = gen_random_uuid();
