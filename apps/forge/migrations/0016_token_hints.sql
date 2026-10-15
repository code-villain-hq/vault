-- The last four characters of each agent token, shown so people can tell
-- their keys apart. Far too few to help anyone guess a token.
ALTER TABLE api_tokens ADD COLUMN hint text;
