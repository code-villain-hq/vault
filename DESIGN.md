---
name: Code Villain
description: Nightshade — grey does the work, cloak is the quiet brand, poison is the accent. Night by default, day for apps.
colors:
  bg: "#0A0B0D"
  panel: "#12131A"
  surface: "#191A23"
  hover: "#1B1C26"
  selected: "#1A0D24"
  line: "#2A2B36"
  fg: "#ECECF1"
  muted: "#9B9BA8"
  faint: "#868693"
  primary: "#331C40"
  primary-hover: "#3A1A49"
  on-primary: "#ECECF1"
  accent: "#C9E265"
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
    fontSize: "0.9375rem"
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
components:
  button-primary:
    backgroundColor: "{colors.primary}"
    textColor: "{colors.on-primary}"
    height: "28px"
    padding: "0 0.7rem"
    rounded: "6px"
  button-primary-hover:
    backgroundColor: "{colors.primary-hover}"
    textColor: "{colors.on-primary}"
  button-secondary:
    backgroundColor: "transparent"
    textColor: "{colors.muted}"
    height: "28px"
    padding: "0 0.7rem"
    rounded: "6px"
  button-secondary-hover:
    backgroundColor: "{colors.hover}"
    textColor: "{colors.fg}"
  field:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.fg}"
    rounded: "6px"
  chip:
    textColor: "{colors.muted}"
    rounded: "999px"
  card-panel:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.fg}"
    rounded: "10px"
---

# Design System: Code Villain

This file summarises `brand/` and `design/` for design tools. Those folders are the source; when they change, this follows.

## Overview

**Creative North Star: "The Nightshade Cloak"**

A golden-age comic villain's court, rendered as a calm product surface. The theatre lives in a few large places: Grenze Gotisch titles and the poison strike on the homepage. Everything read or clicked stays plain, quiet, and legible.

Grey does most of the work. Cloak (violet) is the brand, held to the strength of a grey: the main action, selection. Poison (chartreuse) is an accent, never the main thing.

**Key Characteristics:**
- Night by default; day for the apps' light mode. Studio surfaces stay night.
- Tone, not lines: regions separate by steps in tone and space.
- Soft corners: 6px controls, 10px panels, round pills.
- One quiet primary per screen; everything else steps back.
- Poison only for focus, one brand moment per surface, and live dots.

**The mark** is the strike: a gothic V with a poison bar behind it, low and heavy, cut by the letter (`brand/mark/`). It is the surface's one brand moment; app icons are each app's initial built the same way.

Principles: `design/principles.md`. Components: `design/components.md`.

## Colors

Shared names, filled for night and day (full table in `brand/color.md`, numbers in `design/tokens/color.json`).

- **bg / panel / surface**: frame, content, fields. Elevation is a step in tone.
- **fg / muted / faint**: text, secondary text, ids and placeholders. Links are `fg`, bold, underlined.
- **primary**: the main action's fill, a violet tone (`#331C40` night, `#E8DEEB` day).
- **selected**: violet-tinted selection. Day greys carry no violet.
- **accent**: poison. Bright `#C9E265` by night, deep `#4C6A00` by day.
- **danger / ok / warn / info**: status. Ok is teal so it never reads as poison.

### Named Rules
**The Accent Rule.** Poison goes to the focus ring, one brand moment per surface, and live dots. Never links, labels, chips, body copy, or large fills. Never on cloak.

**The Quiet Brand Rule.** Cloak appears as a tone (primary, selected), not as a solid purple block in product UI.

## Typography

**Display Font:** Grenze Gotisch (700, 800). Splash, page titles, section titles.
**Body Font:** Atkinson Hyperlegible Next (400, 600, 700, italic 400). Reading text, labels, buttons, nav, card titles.

Studio roles (splash to code) serve the site, blog and social cards. Interface roles serve app windows: `title` 21px, `ui` 14px, `ui-small` 13px, `control` 13px/600 for buttons and chips.
**Mono Font:** Atkinson Hyperlegible Mono (400). Code and terminal text.

The gothic is the costume; Atkinson is the person inside it. Source: `brand/type.md`.

### Named Rules
**The 28px Floor.** Grenze never goes below 28px. Under that, use Atkinson 700.

**The No-Capitals Rule.** Grenze is never set in capitals. Titles are sentence case.

**The One Splash Rule.** Weight 800 belongs to the splash alone, once per surface.

## Layout

Apps share one frame: sidebar and top bar on `bg`, dimmed; content rises one step onto a rounded `panel` (`design/layouts/app-window.md`). Prose keeps a 66ch measure. No space scale is published yet; use rem rhythm consistently.

## Elevation & Depth

Flat. Depth is tone: bg → panel → surface. No shadows or borders to lift a surface, and no decorative glow. Field focus is the exception: an accent border plus a 3px accent glow at 22%, as specified in `design/components.md`. The `line` token is for tables and menus only.

## Shapes

`radius-sm` 4px (key hints), `radius-md` 6px (buttons, fields, rows, nav, notes), `radius-lg` 10px (panels, dialogs), `radius-full` (pills, chips).

## Components

- **App button**: 28px, `control` role, no border. Primary: `primary` fill, one per screen. Secondary: muted text, fill only on hover. Focus: 2px accent ring, 2px out.
- **Studio call to action**: `label` role (15px/700, leading 1.2, tracking 0.01em), `0.7rem 1.15rem` padding, automatic height, 6px corners. Same primary colours and no border; focus is a 2px accent ring, 3px out.
- **Field**: `surface` fill (`panel` in the frame), no border; focus draws an `accent` border plus a 3px accent glow at 22%.
- **Status pill**: round; status colour as text and dot on a 13% tint.
- **Chip**: round; muted text on `fg` at 7%.
- **Row**: no dividers; hover and selection are tone steps.
- **Live dot**: small round accent dot for something happening now.

## Do's and Don'ts

### Do:
- **Do** lead each screen with one quiet primary.
- **Do** separate regions by tone and space.
- **Do** keep links in ink, bold and underlined.
- **Do** meet WCAG AA on every text pairing, in night and day.

### Don't:
- **Don't** colour links, chips, or labels with poison.
- **Don't** use solid cloak as a button fill in product UI.
- **Don't** add hairlines between rows or regions.
- **Don't** set Grenze Gotisch below 28px or in capitals.
- **Don't** add shadows or decorative glows; the specified field-focus glow is allowed.
