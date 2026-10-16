# Principles

How Villain designs a screen. Every UI change names the principle it serves. "Another product does it" is not a reason.

Linear's account of its redesign is the reference for the thinking, not for the look: [A design reset](https://linear.app/now/a-design-reset) and [How we redesigned the Linear UI](https://linear.app/now/how-we-redesigned-the-linear-ui).

## Restraint

1. **Show less.** Hide what every row shares. Leave out empty counts. Each screen leads with the one action its state needs; the other actions stay quiet until hovered or asked for.
2. **Tone, not lines.** Regions separate by steps in tone and by space. Rows have no dividers. Pills, chips, and notes are tinted, not outlined. The `line` token is for tables and menus.
3. **Chrome dims, content leads.** The sidebar and headers sit a step back from the content they control. Few text sizes; hierarchy comes from weight and tone.
4. **Even colour.** Colours are compared by OKLCH lightness, so colours at one level look equally bright. Colour carries attention: a calm state stays quiet, and status colour is spent only where something needs a look.
5. **Small details, everywhere.** Labels, icons, and edges line up across the sidebar, tabs, and headers. Checked, not assumed.

## The system

6. **The accent stays out of the neutrals.** Greys carry at most a trace of the base hue. Cloak and poison stay the only colours that mean something.
7. **Elevation is lightness.** Frame, panel, surface, and dialog are steps in lightness. No shadows or borders to lift a surface.
8. **Display for headings, plain for everything else.** Grenze Gotisch carries large titles; Atkinson Hyperlegible carries everything read or clicked ([`brand/type.md`](../brand/type.md)).
9. **One frame for every app.** Forge and Layer share the frame that controls the content: the sidebar and the top bar. How the sidebar, tabs, app header, and view header behave is written once, in [`layouts/`](layouts).

## How a look changes

10. **Concept, then stress test.** A new direction is tried on every main view of every app before it is built: list, board, detail, inbox, settings, empty states.
11. **Ship in one sweep.** A look changed one view at a time leaves the product disjointed. Change it everywhere at once, then polish.
