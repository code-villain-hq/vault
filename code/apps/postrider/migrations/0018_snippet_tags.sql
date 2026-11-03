-- Previews leave out HTML tags and read entities as what they stand for
-- now (`preview.rs`): do the same to the lines kept before, for the tags
-- and the entities mail uses most.
CREATE FUNCTION pg_temp.untagged(snippet text) RETURNS text
LANGUAGE sql IMMUTABLE AS $$
  SELECT btrim(regexp_replace(
    replace(replace(replace(replace(replace(replace(replace(replace(replace(replace(replace(
      regexp_replace(
        regexp_replace(
          snippet,
          '</?(a|abbr|b|big|code|em|font|i|mark|s|small|span|strike|strong|sub|u)([[:space:]/][^<>]*)?>',
          '', 'gi'),
        '</?[A-Za-z][A-Za-z0-9-]*([[:space:]/][^<>]*)?>', ' ', 'g'),
      '&nbsp;', ' '), '&lt;', '<'), '&gt;', '>'), '&quot;', '"'), '&#39;', ''''),
      '&apos;', ''''), '&#8217;', '’'), '&rsquo;', '’'), '&ndash;', '–'), '&mdash;', '—'),
      '&amp;', '&'),
    '[[:space:]]+', ' ', 'g'))
$$;

UPDATE inbox_mails
SET snippet = pg_temp.untagged(snippet)
WHERE snippet ~ '</?[A-Za-z][A-Za-z0-9-]*([[:space:]/][^<>]*)?>|&(nbsp|lt|gt|quot|#39|apos|#8217|rsquo|ndash|mdash|amp);';

UPDATE filed_mails
SET snippet = pg_temp.untagged(snippet)
WHERE snippet ~ '</?[A-Za-z][A-Za-z0-9-]*([[:space:]/][^<>]*)?>|&(nbsp|lt|gt|quot|#39|apos|#8217|rsquo|ndash|mdash|amp);';
