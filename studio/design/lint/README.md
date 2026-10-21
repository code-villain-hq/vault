# Design lint

The guard that keeps every web surface on the design system. CI runs it in the `design` workflow; run it before pushing a UI change:

```sh
bun install && bun run check
```

- **CSS** ([`stylelint.config.mjs`](stylelint.config.mjs), stylelint): no hex colours, named colours or literal colour functions, and colours, backgrounds, type sizes, faces and radii take a design token through `var()`. `color-mix()` over tokens is the tint recipe and passes. It covers `studio/design/components.css`, the site, and Forge's, Layer's and Glyph's CSS.
- **Markup** ([`check-classes.ts`](check-classes.ts)): no Tailwind arbitrary colours, type sizes or radii (`text-[12px]`, `bg-[#111]`, `rounded-[3px]`), no Tailwind palette colours (`bg-zinc-800`, `text-white`), and no literal colours in inline styles, in Forge's, Layer's and Glyph's TSX. The site's HTML isn't checked yet.

Exempt on purpose: the token copies synced into the apps (their `sync-design.ts --check` covers them), Layer's `theme.css` (the palettes its extra themes and profile colours fill the token names with), and Layer's phone page (the mobile redesign is deferred). A line that must keep a literal says why, with `design-lint-ignore` in a comment.

A missing value is a design question, not a lint one: add the role or token in `studio/design/tokens` (as `wordmark` was), then use it.
