# Forge accessibility follow-up

## Done

- [x] Name every navigation landmark.
- [x] Put page headings inside one shared main landmark without changing the visible layout.
- [x] Fix focus return for creation dialogs, New issue and the command menu.
- [x] Finish command-menu selection and contrast verification in night and day.
- [x] Rerun the full 28-state axe audit and keyboard checks.
- [x] Run shared theme tests and `bun run check` on the final changes.
- [x] Commit the approved theme generator and accessibility pass.

## Final results (Oct 17)

axe-core 4.13.0, run in the in-app browser against the dev server, over the
same 28 states as the earlier pass: 12 desktop states in each mode plus
inbox and spec at 320px in each mode. Zero violations in every state. The
landmark warnings (`landmark-unique`, `region`) are gone.

Incompletes needing interpretation, both reviewed and not failures:

- `aria-hidden-focus` in the three dialogs points at Base UI's focus-guard
  spans. Keyboard checks confirm focus stays inside the open dialog.
- `color-contrast` in the command menu covers rows scrolled out of view,
  where axe can't read a background. Measured on their real composites:

| Command menu | Night | Day |
| --- | --- | --- |
| Active row vs dialog | 1.23:1 (was 1.0) | 1.18:1 |
| Active label / hint | 11.47 / 4.62 | 15.1 / 5.51 |
| Normal label / hint | 14.14 / 5.70 | 17.76 / 6.48 |
| Group label | 5.70 | 6.48 |

Focus return after keyboard open and Escape confirmed in day and night for
New team, New issue, and Search and commands, with a visible ring.

## What changed in this last step

- Shared `highlight-tint` (8% `fg`, translucent) for the active option in
  menus and command lists. In night, solid `hover` equals `dialog`, so the
  active row was invisible. Used by the command menu, picker menus and
  @-mentions. Night `faint` moved #939393 → #969696 to stay readable on it.
- Shared `signal` role for live dots: poison held at WCAG's 3:1 non-text
  floor instead of the 4.6:1 text floor. Day #7C9000 instead of #607000.
- Sign-in mark enlarged to 80px. The sidebar tile beside the wordmark is
  removed, as `design/layouts/app-window.md` specifies the wordmark only.

## Still separate

Mobile redesign, token-drift automation and physical-device/screen-reader
testing. Automated axe results are not a screen-reader sign-off.
