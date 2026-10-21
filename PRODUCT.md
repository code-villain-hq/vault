# Product

## Platform

web

## Users

Two audiences share the studio:

1. **Future employers and peers** watching the public build — they should see craft and growth from senior front-end work into shipping whole products with AI.
2. **People using the products** — Forge (AI-first issue tracker), Layer (macOS agent development environment), Caster (macOS launcher), Glyph (AI translation for i18n files), and the Code Villain editor and terminal theme.

Studio surfaces (site, blog, social, mark) speak to the first audience. Apps serve the second. Company-wide design still has to hold for both.

## Product Purpose

Code Villain builds real tools and shows the build in public. Success means a coherent studio record — products shipping, a consistent look and voice, and a growth trail that does not need a job title announced.

The vault is the home for that record: apps, brand, design tokens, and content.

## Positioning

A one-person studio that ships real tools while narrating the build in public — theatrical villain posture without corporate cruelty. Neighboring “AI portfolio” or “indie SaaS” stories cannot truthfully claim the same mix of shipped products (Forge, Layer, Caster, Glyph, the Code Villain theme), Doom-inspired voice, and Nightshade brand system as one company.

## Operating Context

- Brand decisions start in `brand/` (color, type, mark, voice); `design/` carries numbers and layouts; apps catch up in their own toolkits.
- Studio surfaces follow `brand/` and stay night. Nightshade is every app's default, night and day; a theme picker like Layer's can add themes on the same colour names. Every app uses the shared tokens, with drift checks in CI.
- Content lives in `content/blog` and `content/social`; the site renders the blog from them (`apps/site/build.py`).
- Agents read `AGENTS.md` at the vault root and each app’s `AGENTS.md` before working there.
- Role label in public writing: product engineer (current name; may change). The work carries the story, not the title.

## Capabilities and Constraints

**Shipped / in vault**

- Forge — AI-first issue tracker (web + desktop shell).
- Layer — agent development environment for macOS.
- Caster — launcher for macOS.
- Code Villain theme — VS Code and Cursor, Neovim, WezTerm and a Starship prompt, generated from the tokens.
- Glyph — AI translation tool for i18n JSON files, on local models.
- Site — the homepage and the blog (index and post pages, built from `content/blog`).

**Company-wide design**

- Required: one design system for the studio and every app: the same tokens, components and frame, so every product looks and feels the same.
- Apps implement shared decisions in their own stacks; extra themes fill the same colour names.

**Undecided**

- A layout for the social card (`design/layouts/`).
- A spacing scale (`design/tokens/`).

## Brand Commitments

- **Name:** Code Villain. Website: https://www.codevillain.eu/. Products: Forge, Layer, Caster, Glyph; the editor theme is called Code Villain. Source: `brand/name.md`.
- **Voice:** A golden-age comic villain who ships; Doctor Doom posture — theatrical, precise, a little too sure of himself. Code Villain speaks and names itself (“Code Villain shipped Forge”), never “I” or “we”. Lead with the work; boast only what shipped; one flourish per piece. Never mean, never corporate-evil. Same voice for UI, blog, and social. Short surfaces stay short. Source: `brand/voice.md`.
- **Color (Nightshade):** Night `bg` `#0B0B0B`, poison `#C9E265`, cloak `#5A1470`. Grey does most of the work, achromatic in both modes. Poison is the brand: the strike, the main action's fill once per screen, the selection tint, live signals, the search match in code. Cloak supports it: the focus ring and depth. Night by default, day for the apps' light mode; studio surfaces stay night. Source: `brand/color.md`, `design/tokens/`.
- **Mark:** the strike, a gothic C and V with a poison bar behind them, cut where the letters cross. App icons are each app's initial built the same way; a menu bar glyph is the mark without its tile, in colour. Source: `brand/mark/`.
- **Type:** Grenze Gotisch for display (28px and up, never capitals), Atkinson Hyperlegible Next for reading and interface, Atkinson Hyperlegible Mono for code. Code takes the Nightshade code palette (`design/tokens/syntax.json`). Source: `brand/type.md`, `design/tokens/type.json`.
- **Binding references:** Golden-age comic villainy (Doctor Doom for posture, not green/gold costume colors).

## Evidence on Hand

- Vault README and `AGENTS.md` state studio purpose and structure.
- Brand decision files and `brand/index.html` (Nightshade study).
- Machine-readable tokens in `design/tokens/` (CSS, Swift, JSON).
- App inventories and agent notes under `apps/`.
- Blog and social content directories under `content/`.

Do not invent testimonials, customers, benchmarks, pricing, or press.

## Product Principles

1. **Ship and show** — Public work is the proof; the record of building matters as much as the product.
2. **Studio first, apps second** — How Code Villain feels starts in brand/design; products catch up without losing their own job.
3. **One palette, one voice** — Nightshade and the villain voice are company-wide; colour stays sparse and never muddies the night.
4. **Posture, not cruelty** — Boast only what shipped; never mean; never corporate-evil.
5. **Short surfaces stay short** — Buttons and posts can sound like the same person.

## Accessibility & Inclusion

Studio color rules require WCAG AA minimum for every text pairing, in night and day. Broader product a11y standards per app remain open unless an app already defines its own.
