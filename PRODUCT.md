# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

Two audiences share the studio:

1. **Future employers and peers** watching the public build — they should see craft and growth from senior front-end work into shipping whole products with AI.
2. **People using the products** — Forge (AI-first issue tracker), Villain Layer (macOS agent development environment), and Villain Caster (macOS launcher).

Studio surfaces (site, blog, social, mark) speak to the first audience. Apps serve the second. Company-wide design still has to hold for both.

## Product Purpose

Villain Studios builds real tools and shows the build in public. Success means a coherent studio record — products shipping, a consistent look and voice, and a growth trail that does not need a job title announced.

The vault is the home for that record: apps, brand, design tokens, and content.

## Positioning

A one-person studio that ships real tools while narrating the build in public — theatrical villain posture without corporate cruelty. Neighboring “AI portfolio” or “indie SaaS” stories cannot truthfully claim the same mix of shipped products (Forge, Layer, Caster), Doom-inspired voice, and Nightshade brand system as one company.

## Operating Context

- Brand decisions start in `brand/` (color, type, mark, voice); `design/` carries numbers and layouts; apps catch up in their own toolkits.
- Studio surfaces follow `brand/` and stay night. Nightshade is every app's default, night and day; a theme picker like Layer's can add themes on the same colour names.
- Content lives in `content/blog` and `content/social`; the site shows the blog; post pages are hand-written until there is a build step.
- Agents read `AGENTS.md` at the vault root and each app’s `AGENTS.md` before working there.
- Role label in public writing: product engineer (current name; may change). The work carries the story, not the title.

## Capabilities and Constraints

**Shipped / in vault**

- Forge — AI-first issue tracker (web + desktop shell).
- Villain Layer — agent development environment for macOS.
- Villain Caster — launcher for macOS.
- Site — the homepage and the blog (index and post pages, hand-written for now).

**Company-wide design**

- Required: one studio design system for company surfaces (site, blog, social, mark, brand study).
- Apps implement shared decisions in their own stacks; product-specific themes remain allowed.

**Undecided**

- A layout for the social card (`design/layouts/`).
- How the site builds post pages from `content/blog`.
- A spacing scale (`design/tokens/`).

## Brand Commitments

- **Name:** Villain Studios.
- **Voice:** A golden-age comic villain who ships; Doctor Doom posture — theatrical, precise, a little too sure of himself. Villain speaks and names itself (“Villain shipped Forge”), never “I” or “we”. Lead with the work; boast only what shipped; one flourish per piece. Never mean, never corporate-evil. Same voice for UI, blog, and social. Short surfaces stay short. Source: `brand/voice.md`.
- **Color (Nightshade):** Night `#0A0B0D`, cloak `#5A1470`, poison `#C9E265`. Grey does most of the work; cloak is the quiet brand (the main action's violet tone, selection); poison is an accent only (focus, one brand moment per surface, live dots). Night by default, day for the apps' light mode; studio surfaces stay night. Source: `brand/color.md`, `design/tokens/`.
- **Mark:** the strike, a gothic V with a poison bar behind it, cut by the letter. App icons are each app's initial built the same way. Source: `brand/mark/`.
- **Type:** Grenze Gotisch for display (28px and up, never capitals), Atkinson Hyperlegible Next for reading and interface, Atkinson Hyperlegible Mono for code. Source: `brand/type.md`, `design/tokens/type.json`.
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
2. **Studio first, apps second** — How Villain feels starts in brand/design; products catch up without losing their own job.
3. **One cloak, one voice** — Nightshade and the villain voice are company-wide; accents stay sparse and never muddy the night.
4. **Posture, not cruelty** — Boast only what shipped; never mean; never corporate-evil.
5. **Short surfaces stay short** — Buttons and posts can sound like the same person.

## Accessibility & Inclusion

Studio color rules require WCAG AA minimum for every text pairing, in night and day. Broader product a11y standards per app remain open unless an app already defines its own.
