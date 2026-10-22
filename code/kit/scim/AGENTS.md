# Groups from the Code Villain account: notes for coding agents

Pocket ID (id.codevillain.eu) signs people in to every app. With SCIM it
also sends each app the people and groups its sign-in client allows. This
crate, `code-villain-scim`, takes them at `/api/scim/v2` and turns them into
workspace roles. Forge, Glyph, Raven and Layer use it.

## What it does

- A workspace linked to a group (`workspaces.group_name`, say `forge`) has
  the group's people as members, `forge-admins` as admins and
  `forge-guests` as guests. Someone in more than one has the highest role.
- The role's group is kept in `workspace_members.from_group`. Members shows
  it ("from forge-admins") and refuses to change the role or remove the
  person: that's done in the account.
- Someone who came by invite, and isn't in the groups, stays as they are.
  When a group takes them, it sets their role from then on.
- Out of every group, someone whose role came from one leaves the
  workspace. Deleted or turned off in the account, they also lose their
  sessions; their account here stays.
- Unlinking a workspace keeps everyone with their roles, as though invited.

People are matched to accounts here by their account id (`users.oidc_sub`,
which Pocket ID sends as `externalId`), else by an email no other sign-in
has. Someone without an email is refused, and so is the sync of their groups.

## Using it in an app

- A migration with `schema.sql`, as the app copied the sync engine's.
- `impl Directory for <App>Directory`: `add_member`, `change_role` and
  `remove_member` do what accepting an invite, changing a role and
  "Remove" do in Members, so the app's sync records and its own rules
  follow. `groups_changed` is for what else follows a group (Forge's linked
  teams).
- Mount `code_villain_scim::router::<D, S>(pool, token)` at `/api/scim/v2`
  when `<APP>_SCIM_TOKEN` is set, and only then.
- Members: `role_source` before a role change or a removal; `from_group`
  in the members list; `GET`/`PUT /api/w/{id}/group` for admins with
  `linked_group`, `group_names` and `link_workspace`, not there when the
  token isn't set.
- Forge's `crates/server/src/groups.rs` and `tests/scim.rs` show the whole of it.

## What Pocket ID sends

Read from its source (`backend/internal/scimsync`), since its docs run
ahead of its releases:

- It lists `/Users` and `/Groups` with `startIndex` and `count=1000`, then
  creates (POST), replaces (PUT) or deletes (DELETE) each one. No PATCH,
  no filters. It sends `Content-Type: application/scim+json`, which is why
  bodies are read as bytes.
- Users first, then groups; a group's members are the ids this server gave
  its users.
- A group's `displayName` is its friendly name in Pocket ID, not its name.
  Give every group the same friendly name as its name (`forge-admins`).
- Renaming a linked group unlinks it in effect: its people leave. Link the
  workspace again after a rename.
- It syncs five minutes after a change, every hour, and on "Sync now".
  Responses leave out `meta.lastModified`, so it sends everyone each time
  rather than skipping what it thinks we have.

## Changing it

A change here changes every app that uses it. Check with `bun run check`
in each of them.
