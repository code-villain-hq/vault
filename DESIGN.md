---
name: Code Villain
description: Nightshade. Grey does the work, poison is the brand and the main action, cloak supports it and marks focus. Night by default, day for the apps.
colors:
  bg: "#0B0B0B"
  panel: "#141414"
  surface: "#1B1B1B"
  dialog: "#1D1D1D"
  field: "#1B1B1B"
  hover: "#1D1D1D"
  selected: "#1C1E13"
  line: "#2B2B2B"
  fg: "#EBEBEB"
  muted: "#9B9B9B"
  faint: "#969696"
  primary: "#C9E265"
  primary-hover: "#B6CE50"
  on-primary: "#0B0B0B"
  accent: "#AF6BC8"
  signal: "#C9E265"
  danger: "#F27A6B"
  ok: "#5CC9A7"
  warn: "#E9AE45"
  info: "#7AAEF5"
  cloak: "#5A1470"
  poison: "#C9E265"
typography:
  splash:
    fontFamily: "Grenze Gotisch, ui-serif, Georgia, serif"
    fontSize: "clamp(4.5rem, 14.5vw, 11.5rem)"
    fontWeight: 800
    lineHeight: 0.95
  display:
    fontFamily: "Grenze Gotisch, ui-serif, Georgia, serif"
    fontSize: "clamp(2.25rem, 6vw, 3.5rem)"
    fontWeight: 700
    lineHeight: 1.1
  headline:
    fontFamily: "Grenze Gotisch, ui-serif, Georgia, serif"
    fontSize: "clamp(1.75rem, 3.4vw, 2.125rem)"
    fontWeight: 700
    lineHeight: 1.18
  title:
    fontFamily: "Atkinson Hyperlegible Next, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1.3125rem"
    fontWeight: 700
    lineHeight: 1.25
  label:
    fontFamily: "Atkinson Hyperlegible Next, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1.0625rem"
    fontWeight: 700
    lineHeight: 1.2
    letterSpacing: "0.01em"
  body:
    fontFamily: "Atkinson Hyperlegible Next, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1.0625rem"
    fontWeight: 400
    lineHeight: 1.6
  small:
    fontFamily: "Atkinson Hyperlegible Next, ui-sans-serif, system-ui, sans-serif"
    fontSize: "0.875rem"
    fontWeight: 400
    lineHeight: 1.5
  code:
    fontFamily: "Atkinson Hyperlegible Mono, ui-monospace, Menlo, monospace"
    fontSize: "0.875rem"
    fontWeight: 400
    lineHeight: 1.6
  wordmark:
    fontFamily: "Grenze Gotisch, ui-serif, Georgia, serif"
    fontSize: "1.75rem"
    fontWeight: 700
    lineHeight: 1.1
  query:
    fontFamily: "Atkinson Hyperlegible Next, ui-sans-serif, system-ui, sans-serif"
    fontSize: "1.3125rem"
    fontWeight: 400
    lineHeight: 1.25
  ui:
    fontFamily: "Atkinson Hyperlegible Next, ui-sans-serif, system-ui, sans-serif"
    fontSize: "0.875rem"
    fontWeight: 400
    lineHeight: 1.45
  ui-small:
    fontFamily: "Atkinson Hyperlegible Next, ui-sans-serif, system-ui, sans-serif"
    fontSize: "0.8125rem"
    fontWeight: 400
    lineHeight: 1.4
  control:
    fontFamily: "Atkinson Hyperlegible Next, ui-sans-serif, system-ui, sans-serif"
    fontSize: "0.8125rem"
    fontWeight: 600
    lineHeight: 1
  ui-code:
    fontFamily: "Atkinson Hyperlegible Mono, ui-monospace, Menlo, monospace"
    fontSize: "0.8125rem"
    fontWeight: 400
    lineHeight: 1.6
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
    height: "28px"
    padding: "0 0.75rem"
    rounded: "6px"
  button-primary-hover:
    backgroundColor: "{colors.primary-hover}"
    textColor: "{colors.on-primary}"
  button-secondary:
    backgroundColor: "color-mix(in srgb, {colors.fg} 10%, transparent)"
    textColor: "{colors.fg}"
    height: "28px"
    padding: "0 0.75rem"
    rounded: "6px"
  button-secondary-hover:
    backgroundColor: "color-mix(in srgb, {colors.fg} 16%, transparent)"
    textColor: "{colors.fg}"
  field:
    backgroundColor: "{colors.field}"
    textColor: "{colors.fg}"
    rounded: "6px"
  chip:
    textColor: "{colors.muted}"
    rounded: "999px"
  dialog:
    backgroundColor: "{colors.dialog}"
    textColor: "{colors.fg}"
    rounded: "10px"
  card-panel:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.fg}"
    rounded: "10px"
---

# Design System: Code Villain

This file summarises `studio/brand/` and `studio/design/` for design tools. Those folders are the source. The frontmatter above is generated from `studio/design/tokens` by `python3 build.py` there, and CI fails if it drifts; the prose below follows the same sources.

## Overview

**Creative North Star: "The Nightshade Court"**

A golden-age comic villain's court, rendered as a calm product surface. The theatre lives in a few large places: Grenze Gotisch titles and the poison strike. Everything read or clicked stays plain, quiet, and legible.

Grey does most of the work. Poison (chartreuse) is the brand: the strike in every mark, the main action's fill, the selection tint, and live signals. Cloak (deep violet) supports it with depth and marks focus.

**Key Characteristics:**
- Night by default; day for the apps' light mode. Studio surfaces stay night.
- Tone, not lines: regions separate by steps in tone and space. Greys stay achromatic.
- Soft corners: 6px controls, 10px panels and dialogs, round pills.
- One primary per screen, in poison with dark ink; everything else steps back.
- Every colour at one level shares an OKLCH lightness, so none shouts.

**The mark** is the strike: a gothic C and V with a poison bar behind them, low and heavy, cut where the letters cross (`studio/brand/mark/`). It is the surface's one brand moment. Each app's icon is its initial built the same way; a menu bar glyph is the mark without its tile, in colour.

Principles: `studio/design/principles.md`. Components: `studio/design/components.md`. Behaviour: `studio/design/behaviour.md`.

## Colors

Shared names, filled for night and day (full table in `studio/brand/color.md`, numbers in `studio/design/tokens/`). Themes are calculated from three inputs (`base` poison, `accent` cloak, `contrast`), not hand-tuned lists.

- **bg / panel / surface / dialog**: frame, content, raised notes, dialogs. Elevation is a step in tone.
- **field / dialog-field**: form controls, a tone step from their host.
- **fg / muted / faint**: text, secondary text, ids and placeholders. Links are `fg`, bold, underlined.
- **primary**: poison, the main action's fill, with dark `on-primary` ink.
- **selected**: a faint poison tint for the selected row and current nav item.
- **accent**: cloak, lifted to a readable violet by night: the focus ring.
- **signal**: poison at the 3:1 non-text floor: live dots.
- **danger / ok / warn / info**: status, with `status-<name>-ink` for text on their 13% tints. Ok is teal so it never reads as poison.
- **backdrop-tint**: black at 40% behind a dialog.

### Named Rules
**The Strike Rule.** Poison goes to the strike, the main action (once per screen), the selection tint, live signals, the search match in code, and a terminal's green. Never links, labels, chips, body copy, or large fills.

**The Cloak Rule.** Cloak is focus and depth. It never sits under poison text and never tints the large neutral surfaces.

## Typography

**Display Font:** Grenze Gotisch (700, 800). Splash, page titles, section titles.
**Body Font:** Atkinson Hyperlegible Next (400, 600, 700, italic 400). Reading text, labels, buttons, nav, card titles.
**Mono Font:** Atkinson Hyperlegible Mono (400). Code and terminal text.

Studio roles (splash to code) serve the site, blog and social cards; `label` is 17px. Interface roles serve app windows: `title` 21px/700, `query` 21px/400 for a launcher's or command menu's query, `ui` 14px, `ui-small` 13px, `control` 13px/600 for buttons and chips, `ui-code` 13px.

The gothic is the costume; Atkinson is the person inside it. Source: `studio/brand/type.md`.

### Named Rules
**The 28px Floor.** Grenze never goes below 28px. Under that, use Atkinson 700.

**The No-Capitals Rule.** Grenze is never set in capitals. Titles are sentence case.

**The One Splash Rule.** Weight 800 belongs to the splash alone, once per surface.

## Layout

Apps share one frame: sidebar and top bar on `bg`; content rises one step onto a rounded `panel` inset 0.5rem (`studio/design/layouts/app-window.md`). Prose keeps a 66ch measure. A spacing scale is still a draft (`studio/design/spacing.draft.md`); use rem rhythm consistently.

## Elevation & Depth

Flat. Depth is tone: bg → panel → surface → dialog. No shadows or borders to lift a surface, and no decorative glow. Field focus is the exception: an accent border plus a 3px accent glow at 22%. The `line` token is for tables and menus only.

## Shapes

`radius-sm` 4px (key hints), `radius-md` 6px (buttons, fields, rows, nav, notes), `radius-lg` 10px (panels, dialogs), `radius-full` (pills, chips).

## Components

- **App button**: 28px, `control` role, no border. Primary: poison fill with dark ink, one per screen. Secondary: `fg` text on `fg` at 10% (16% on hover). Icon buttons stay unfilled. Focus: 2px accent ring, 2px out.
- **Button, link, tab, chip**: a button changes something (filled block); a link goes somewhere (underlined, ↗ when it leaves the app); a tab switches the view (text, a 2px `fg` line under the current one); a segmented control picks one value (a track, the chosen option filled); a chip says something (round).
- **Studio call to action**: `label` role (17px/700), `0.7rem 1.15rem` padding, 6px corners, the primary colours, no border.
- **Field**: a tone step with no border; focus draws an accent border plus a 3px accent glow at 22%.
- **Dialog**: `dialog` fill, `radius-lg`, a backdrop of black at `backdrop-tint`.
- **Status pill**: round; the status hue at 13% with its status ink.
- **Chip**: round; muted text on `fg` at 7%.
- **Row**: no dividers; hover and selection are tone steps; a menu's active option is `fg` at 8%.
- **Icon**: one line set per toolkit (Lucide, SF Symbols), 15px beside text, `muted`; never poison or cloak.
- **Live dot**: a small round `signal` dot for something happening now.

## Code

Code uses the Nightshade code palette (`studio/design/tokens/syntax.json`, rules in `studio/design/tokens/theme.md`): six hues at one lightness (violet keywords; cyan properties, operators and built-in types; teal strings; amber constants; blue functions; rose `this` and tags), names in `fg`, greys for brackets, punctuation and comments. The same palette dresses the Code Villain editor theme (`code/apps/theme`: VS Code and Cursor, Neovim, WezTerm, Starship), the apps, and code on the site. Every terminal takes one 16-colour table: the same hues at a calmer chroma, with poison as the green.

## Do's and Don'ts

### Do:
- **Do** lead each screen with one primary.
- **Do** separate regions by tone and space.
- **Do** keep links in ink, bold and underlined.
- **Do** meet WCAG AA on every text pairing, in night and day.

### Don't:
- **Don't** colour links, chips, labels or icons with poison.
- **Don't** let cloak tint surfaces or sit under poison text.
- **Don't** add hairlines between rows or regions.
- **Don't** set Grenze Gotisch below 28px or in capitals.
- **Don't** add shadows or decorative glows; the specified field-focus glow is allowed.
