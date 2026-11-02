# Design lint

The guard that keeps every web surface on the design system. CI runs it in the `design` workflow; run it from `studio/design` before pushing a UI change:

```sh
bun install && bun run check
```

`bun run check` there also formats-checks and lints `studio/design`'s own code and `components.css` with Vite+ (`vite.config.ts`); `bun run format` fixes them.

- **CSS** ([`stylelint.config.mjs`](stylelint.config.mjs), stylelint): stylelint's recommended rules (unknown properties, units and at-rules, duplicates, empty blocks, deprecated values; Tailwind's at-rules allowed). Then no hex colours, named colours or literal colour functions, and colours, backgrounds, type sizes, faces and radii take a design token through `var()`. `color-mix()` over tokens is the tint recipe and passes. It covers `studio/design/components.css`, the site, and Forge's, Layer's (its server's web app too), Glyph's, Raven's, Almanac's and Postrider's CSS. Vite+ formats CSS but doesn't lint it, so this is the CSS linter.
- **Markup** ([`check-classes.ts`](check-classes.ts)): no Tailwind arbitrary colours, type sizes or radii (`text-[12px]`, `bg-[#111]`, `rounded-[3px]`), no Tailwind palette colours (`bg-zinc-800`, `text-white`), and no literal colours in inline styles, in Forge's, Layer's, Glyph's, Raven's, Almanac's and Postrider's TSX. The site's HTML isn't checked yet.

Exempt on purpose: the token copies synced into the apps (their `sync-design.ts --check` covers them), Layer's `theme.css` (the palettes its extra themes and profile colours fill the token names with), and Layer's phone page (the mobile redesign is deferred). A line that must keep a literal says why, with `design-lint-ignore` in a comment.

A missing value is a design question, not a lint one: add the role or token in `studio/design/tokens` (as `wordmark` was), then use it.
