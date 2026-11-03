-- Previews leave out the [1] a text part numbers its links with now: take
-- it out of the lines kept before too.
UPDATE inbox_mails
SET snippet = btrim(regexp_replace(snippet, '\s*\[[0-9]+\]', '', 'g'))
WHERE snippet ~ '\[[0-9]+\]';

UPDATE filed_mails
SET snippet = btrim(regexp_replace(snippet, '\s*\[[0-9]+\]', '', 'g'))
WHERE snippet ~ '\[[0-9]+\]';
