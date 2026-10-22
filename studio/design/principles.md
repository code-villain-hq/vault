# Principles

How Code Villain designs a screen. Every UI change names the principle it serves. "Another product does it" is not a reason.

Linear's account of its redesign is the reference for the thinking, not for the look: [A design reset](https://linear.app/now/a-design-reset) and [How we redesigned the Linear UI](https://linear.app/now/how-we-redesigned-the-linear-ui).

## Restraint

1. **Show less.** Hide what every row shares. Leave out empty counts. Each screen leads with the one action its state needs; the other actions stay quiet until hovered or asked for.
2. **Tone, not lines.** Regions separate by steps in tone and by space. Rows have no dividers. Pills, chips, and notes are tinted, not outlined. The `line` token is for tables and menus.
3. **Chrome dims, content leads.** The sidebar and headers sit a step back from the content they control. Few text sizes; hierarchy comes from weight and tone.
4. **Even colour.** Colours are compared by OKLCH lightness, so colours at one level look equally bright. Colour carries attention: a calm state stays quiet, and status colour is spent only where something needs a look.
5. **Small details, everywhere.** Labels, icons, and edges line up across the sidebar, tabs, and headers. Checked, not assumed.

## The system

6. **The accent stays out of the neutrals.** Greys stay achromatic in both modes. Base and accent colours never tint the large neutral surfaces. Cloak and poison are the only brand colours. The four status colours (ok, warn, danger, info) each name a state and are never decoration.
7. **Elevation is lightness.** Frame, panel, surface, and dialog are steps in lightness. No shadows or borders to lift a surface.
8. **Display for headings, plain for everything else.** Grenze Gotisch carries large titles; Atkinson Hyperlegible carries everything read or clicked ([`studio/brand/type.md`](../brand/type.md)).
9. **One frame for every app.** Forge, Layer, Glyph and Raven share the frame that controls the content: the sidebar and the top bar. The sidebar, top bar, and view header are defined in [`layouts/`](layouts); Layer's desktop window keeps a full-width top bar with its views as tabs, an exception recorded there. Tabs are defined in [`components.md`](components.md#tabs); shared app-header behavior is not defined yet.

## How a look changes

10. **Concept, then stress test.** A new direction is tried on every main view of every app before it is built: list, board, detail, inbox, settings, empty states.
11. **Ship in one sweep.** A look changed one view at a time leaves the product disjointed. Change it everywhere at once, then polish.
