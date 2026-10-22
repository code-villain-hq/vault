# Behaviour

How every Code Villain app acts, so that Forge, Glyph, Raven, Layer and Caster feel like one studio's work. [`components.md`](components.md) covers how controls look. This page covers what the apps do: how you sign in, what a failure says, what an empty list offers, which keys work, and where settings live.

Forge is the reference. Where another app has the same thing, it does it the same way, in its own toolkit. An app that doesn't have the thing (Caster has no sign-in) skips the section. The site isn't an app and follows [`studio/brand/`](../brand/).

## Words

- Sentences, not labels. A capital letter, a full stop, plain words: "That took too long. Try again."
- A problem says what happened, then what to do. With no better fix, the second sentence is "Try again." Unknown problems read "Something went wrong. Try again."
- Losing the server reads "Can't reach Forge. Check your connection.", with the app's own name.
- A busy button keeps its place and changes its label: "Sign in" becomes "Signing in…". A copy button reads "Copied" for a moment.
- Times are relative: "just now", "5 minutes ago", "yesterday", then a date ("Oct 3"). A state says how long it has lasted ("Down for 12 minutes"), with the clock on hover. Where the exact moment is the point, in a log line or a call's timeline, the time is a clock.

## Signing in

One Code Villain account opens every app that has accounts. [`AGENTS.md`](../../AGENTS.md#signing-in-with-code-villain) says how to wire it.

- The sign-in page is titled "Sign in to <App>". With the Code Villain account set up, it holds one primary button, "Sign in with Code Villain ↗" (it opens another site), and the line "One account for every Code Villain app. New here? Ask your team for an invite." Passwords are off then.
- The desktop app signs in through the browser. It says "Finish signing in in your browser." while it waits.
- The browser then lands on a page that hands the sign-in back to the app. It says "You're signed in. <App> is opening. You can close this tab.", with a button to open the app. When sign-in fails, the page says "Couldn't sign you in", gives the reason, and tells the app too, so the app stops waiting and shows the same reason.
- Signing out sits in the workspace menu, the command menu and Account. It removes the app's copy of your workspaces from the device, and Account says so.
- A signed-out visit goes to the sign-in page and comes back to where it started.

## Who gets in

Who belongs where is set once, in the Code Villain account's groups, not app by app. It covers workspaces on a server; workspaces on this Mac have no accounts and stay as they are.

- A workspace on a server can be linked to a group, in its settings under Members. Everyone in `code-villain` is then a member of it, in every app whose workspace is linked to `code-villain`. Everyone in `code-villain-admins` is an admin there, and everyone in `code-villain-guests` a guest. Someone in more than one has the highest role.
- Roles are admin, member and guest. A guest sees only what they've been added to, and works there as a member does: teams in Forge, languages in Glyph, apps in Raven, repositories in Layer. A guest added to nothing sees an empty workspace that says "Ask an admin to add you to a team" (or language, app, repository).
- In Forge a team can be linked to a group too, in the team's settings. Its people are the team's members: it's in their sidebar under "Your teams", and a guest in it sees that team.
- People arrive before they sign in. Members lists them as soon as they're in the group, so they can be assigned or mentioned on day one. Their first sign-in just opens the workspace.
- A role that comes from a group is changed in the Code Villain account. Members says where it comes from ("from code-villain-admins") and has no role menu for it.
- Leaving the group takes the person out of the workspace within minutes, and signs them out of it.
- Invites keep working, for people outside the group. Their role is changed in Members, as now.
- The sign-in page's line stays "New here? Ask your team for an invite.": being added to the group counts as one.

## Errors

- A failure of something you just did shows where you did it: inline, in the danger tint, as an alert. Not a toast.
- Something that fails in the background shows in the frame's status, where the sync status sits: "Change refused: …" for a few seconds.
- The app retries on its own when it can, and says it is waiting. There's no "Retry" button for a connection.

## When something worked

- A change you can see where you made it says nothing more: a saved field shows its new value, a new row appears. No toast, no "Saved." message.
- A toast is for news from out of view: an agent finishing in another task, a sign-in finishing in the browser.

## Empty and loading

- An empty list names what would be there and how it gets there: "No services yet. A service is …". When the view has one action that fills it, the empty state leads with that action as the primary button.
- A filtered list with no matches says so: "No issues match these filters." A place with nothing reads "Nothing here."
- Starting up shows the app's mark, breathing, and nothing else. No spinners or skeletons in views: content appears when it's ready.

## Confirming

- Only what can't be undone asks first. Everything else just happens.
- The question is a dialog, never the system's `confirm`, which webviews drop. Cancel has the focus, so Enter is safe. The danger button names the action ("Remove agent"), not "OK" or "Delete".
- The text says who it affects and ends "This can't be undone."

## Keys

- ⌘K opens the command menu: actions on what's in front of you, create, go to, settings, workspaces, sign out, and search. In Caster the whole app is that menu, on ⌘Space.
- `?` opens the list of shortcuts. Every shortcut an app has is on that list.
- G then a letter goes to a place. C creates the app's main thing. J and K (and the arrows) move through a list. Esc goes back or closes. ⌘Enter sends or saves what you're writing.
- Shortcuts don't fire while you type or while a dialog or menu is open.
- In the desktop apps, ⌘, opens settings.

## Settings

- One place, in the app, split into the workspace's settings and yours (Account).
- A field saves when you leave it or press Enter. No Save button.
- The exception is a form that only works as a whole, such as a connection's address and token: the app has to try them together. It keeps one button, named by what it does: "Connect" the first time, "Reconnect" after.
- Night or day follows the system, and night is the default. An app with a theme picker opens on Nightshade, following the system.
- Settings ends with the app's name and version ("Forge 0.1.0").

## Sync

The sync status sits at the foot of the sidebar, a dot and a word:

- "Synced" when all is saved.
- "Syncing 3" when changes are on their way, shown only after a second.
- "Connecting…" while the app finds the server.
- "Offline · 3 waiting" when it can't, with what it will send once it can.
- "Change refused: …" for a change the server turned down.

## Desktop

- The window follows [`layouts/app-window.md`](layouts/app-window.md): hidden title, traffic lights at the same spot in every app.
- Every link that leaves the app opens in the browser, caught in one place, not link by link.
- The app opens on its own `<app>://` link, which the browser uses to hand back the sign-in.
- Sign-in tokens live in the keychain, one entry per server.
- Notifications come from the inbox, and the dock badge counts what's unread.
