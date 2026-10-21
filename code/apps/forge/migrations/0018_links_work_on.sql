-- Whether a pull request or commit works on the issue (GitHub's rule: the
-- issue is in its branch name or follows a closing word) or only mentions it.
-- The review bar offers only pull requests that work on the issue.
--
-- Pull requests linked before this was recorded keep counting as working on
-- their issues, as the review bar treated them until now; their next webhook
-- event sets it properly. Older commit links can't be told apart; nothing
-- uses this for commits yet.
ALTER TABLE issue_links ADD COLUMN works_on boolean NOT NULL DEFAULT false;
UPDATE issue_links SET works_on = true WHERE kind = 'pull_request';

UPDATE workspaces SET sync_epoch = gen_random_uuid();
