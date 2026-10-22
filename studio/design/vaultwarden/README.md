# Vaultwarden

Vaultwarden at password.codevillain.eu is the studio's password manager.
Its web vault is Bitwarden's, and it isn't ours to rebuild. It does read
one stylesheet of our own after its own, so the vault wears Nightshade in
night and day.

`python3 build.py` writes `kit/user.vaultwarden.scss.hbs` from
`studio/design/tokens/color.resolved.json` and `studio/brand/mark/`.
`python3 build.py --check` writes nothing and fails when the kit is stale,
as it is after a token or mark change.

## What it sets

- **Grounds:** the sidebar and page are `bg`, content sits on `panel`, lines
  are `line`. Greys only.
- **Poison:** the main action's fill with `on-primary` ink, and the faint
  `selected` tint. Nothing else.
- **Ink:** names, links and the current filter are `fg`, bold; text links
  are underlined.
- **Focus:** a focused field rings in cloak (`accent`).
- **Status:** success, danger and warning take their seeds, with
  `status-tint` fills and the status inks on them.
- **Mark:** the Code Villain wordmark replaces Vaultwarden's, in its day and
  night versions; the folded sidebar shows the tile.

The web vault names every colour twice: day in `:root`, night in
`.theme_dark`, as semantic names (`--color-bg-primary`) and older bare
channels (`--color-primary-600: 201 226 101`). The build sets each name in
both blocks. Ours load after Bitwarden's, so a name set only in `:root`
would win at night too.

## Putting it on the server

Copy `kit/user.vaultwarden.scss.hbs` into `templates/scss/` in Vaultwarden's
data folder (or under `TEMPLATES_FOLDER`, when that is set) and restart
Vaultwarden. The runbook for the studio's server is in the infra repository.

Vaultwarden compiles the file as SCSS on start and serves it at
`/css/vaultwarden.css`.
`RELOAD_TEMPLATES=true` re-reads it without a restart; leave that off in
production. To go back to Bitwarden's look, delete the file and restart.

## What stays Bitwarden's

- Fonts, radius, spacing and layout.
- Illustrations, item icons and the empty-state art.
- The browser extension and the desktop and phone apps. They don't load
  this stylesheet.
- The class names it targets. A web-vault update can rename them, so after
  one, open the vault in night and day and check the main action, a focused
  field, the current filter and the logo.
