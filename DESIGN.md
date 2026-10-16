---
name: Villain Studios
description: Nightshade studio system — cloak fill, poison mark, on night.
colors:
  night: "#0A0B0D"
  cloak: "#5A1470"
  cloak-hover: "#6B1884"
  poison: "#C9E265"
  wash: "#1A0D24"
  ink: "#ECECF1"
  muted: "#9B9BA8"
  panel: "#12131A"
  line: "#2A2B36"
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
components:
  button-primary:
    backgroundColor: "{colors.cloak}"
    textColor: "{colors.ink}"
    padding: "0.55rem 0.9rem"
  button-primary-hover:
    backgroundColor: "{colors.cloak-hover}"
    textColor: "{colors.ink}"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    padding: "0.55rem 0.9rem"
  button-ghost-hover:
    backgroundColor: "{colors.wash}"
    textColor: "{colors.ink}"
  chip:
    textColor: "{colors.poison}"
    padding: "0.35rem 0.65rem"
  card-panel:
    backgroundColor: "{colors.panel}"
    textColor: "{colors.ink}"
    padding: "1rem 1.1rem"
---

# Design System: Villain Studios

## Overview

**Creative North Star: "The Nightshade Cloak"**

Villain Studios’ visual system is theatrical, sharp, grand, and imperious — a golden-age comic villain’s court rendered as a dark product surface. The world sits on near-black night. Deep violet (Cloak) is the authoritative fill: buttons, brand mass, armor. Chartreuse (Poison) is the rare mark: a link, a focus ring, a 3px edge — never the page’s voice.

Depth comes from tonal steps (night → panel → wash) and hard hairlines, not glow or soft shadow. Corners stay square. Grenze Gotisch carries the large titles; Atkinson Hyperlegible Next carries everything read or clicked. Apps may keep product themes; studio surfaces follow Nightshade.

**Key Characteristics:**
- Night ground; Cloak fill; Poison mark
- Flat, tonal, square — no accent glows
- One Poison accent per screen
- Imperious density: sparse chrome, clear hierarchy
- Poison never on Cloak as text; never as body copy

## Colors

Nightshade is a two-note brand on a neutral night stack: violet authority beside chartreuse signal.

### Primary
- **Cloak** (`#5A1470`): Deep violet. Primary fills — buttons, brand mass, ghost borders, chip borders. Hover sibling **Cloak Hover** (`#6B1884`).
- **Poison** (`#C9E265`): Chartreuse. Links, focus outlines, 3px card edges, chips. One accent per screen.

### Neutral
- **Night** (`#0A0B0D`): Every studio ground.
- **Panel** (`#12131A`): Cards and raised surfaces.
- **Wash** (`#1A0D24`): Selected rows and hover backgrounds (violet-tinted, not a purple slab).
- **Ink** (`#ECECF1`): Body text on night and panel; labels on Cloak fills.
- **Muted** (`#9B9BA8`): Secondary text.
- **Line** (`#2A2B36`): Hairline borders.

### Named Rules
**The Cloak-and-Poison Rule.** Cloak is the fill. Poison is the mark. Poison never carries body copy and never sits on the cloak.

**The One Mark Rule.** Use at most one Poison accent treatment per screen (link cluster, edge, or focus — not all competing).

**The Muddy-Fill Ban.** Do not paint large surfaces with deep violet on night; keep grounds Night/Panel and let Cloak stay chrome and fill for controls.

## Typography

**Display Font:** Grenze Gotisch (700, 800). Splash, page titles, section titles.
**Body Font:** Atkinson Hyperlegible Next (400, 700, italic 400). Reading text, labels, buttons, nav, card titles.
**Mono Font:** Atkinson Hyperlegible Mono (400). Code and terminal text.

**Character:** The gothic is the costume; Atkinson is the person inside it. Blackletter carries the theatre in a few large places. Everything meant to be read or clicked stays plain and legible. Source: `brand/type.md`; numbers in `design/tokens/type.json`.

### Hierarchy
- **Splash** (Grenze 800, `clamp(4.5rem, 14.5vw, 11.5rem)`, 0.95): Cover lettering. One per surface.
- **Display** (Grenze 700, `clamp(2.25rem, 6vw, 3.5rem)`, 1.1): Page and post titles.
- **Headline** (Grenze 700, `clamp(1.75rem, 3.4vw, 2.125rem)`, 1.18): Section titles.
- **Title** (Atkinson 700, `1.3125rem`, 1.25): Card titles, product names.
- **Label** (Atkinson 700, `0.9375rem`, 1.2, tracking `0.01em`): Buttons, chips, nav.
- **Body** (Atkinson 400, `1.0625rem`, 1.6): Reading text; measure 66ch.
- **Small** (Atkinson 400, `0.875rem`, 1.5): Captions, dates, meta.
- **Code** (Atkinson Mono 400, `0.875rem`, 1.6): Code blocks, terminal text.

### Named Rules
**The 28px Floor.** Grenze never goes below 28px. Under that, use Atkinson 700.

**The No-Capitals Rule.** Grenze is never set in capitals. Titles are sentence case.

**The One Splash Rule.** Weight 800 belongs to the splash alone, once per surface.

## Layout

Studio brand study uses a centered measure (`max-width` ~72rem main, ~66ch prose), horizontal padding `clamp(1.25rem, 4vw, 3rem)`, and section rhythm with large top padding (`~5.5rem`). Action rows use flex wrap with gap `0.75rem 1rem`. Swatch pairs are two equal columns collapsing to one under ~800px. No formal spacing token scale is published yet — use rem rhythm consistently rather than inventing a scale name.

## Elevation & Depth

Flat by default. No box-shadow vocabulary in the Nightshade brand system. Depth is tonal (Night → Panel → Wash) plus 1px Line/Ink borders and the 3px Poison left edge on emphasized cards.

### Named Rules
**The No-Glow Rule.** No gradients, glows, or soft shadows on the accent. Poison is hard edge and ink, not bloom.

## Shapes

Square corners throughout the documented Nightshade components. No border-radius token. Forms read as cut armor plates, not pills.

### Named Rules
**The Square Armor Rule.** Prefer hard corners for studio chrome until a radius scale is deliberately chosen.

## Components

Controls feel sharp and theatrical — imperious, not soft SaaS.

### Buttons
- **Shape:** Square (0 radius).
- **Primary:** Cloak fill, Ink label, 1px Poison border; padding `0.55rem 0.9rem`; weight 700. Hover → Cloak Hover. Focus-visible → 2px Poison outline, offset 3px.
- **Ghost:** Transparent, Ink label, 1px Cloak border. Hover → Wash background.

### Chips
- **Style:** Poison text, Cloak border, transparent fill; padding `0.35rem 0.65rem`; weight 700.
- **State:** Mark-only; do not fill chips with Poison.

### Cards / Containers
- **Corner Style:** Square.
- **Background:** Panel.
- **Shadow Strategy:** None; flat.
- **Border:** 1px Line; emphasized cards add 3px Poison left edge.
- **Internal Padding:** ~`1rem 1.1rem`.

### Navigation
- Brand study nav is Ink on Night with underline current state — not Poison (Poison is for content links, not nav).

### Links
- Poison, underline, weight 600; underline offset ~0.22em. Content/action links only.

## Do's and Don'ts

### Do:
- **Do** put Ink (`#ECECF1`) on Night or Panel for body, and Ink on Cloak for primary button labels.
- **Do** use Poison for links, focus, 3px edges, and chips — sparsely.
- **Do** keep surfaces Night / Panel / Wash; reserve Cloak for fills and borders.
- **Do** meet WCAG AA minimum (AAA for body) on text pairings.

### Don't:
- **Don't** put Poison text on a Cloak fill.
- **Don't** use Poison for headings, body copy, or nav links.
- **Don't** use Poison as a large background fill.
- **Don't** set Grenze Gotisch below 28px or in capitals.
- **Don't** invent radii or accent glows ahead of brand decisions.
- **Don't** treat app theme accents (Layer lavender, Forge indigo) as the studio brand.
