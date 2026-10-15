# Color

This file is the studio palette. It is not chosen yet.

The apps currently use two different ones:

| | Background | Accent |
| --- | --- | --- |
| Forge | `#f4f4f6` light, `#0c0c0e` dark | `#5b5bd6` light, `#8080f2` dark |
| Layer and Caster | `#0c0c11` | `#a78bfa`, with `#4ade80` for the mask's eyes |

Forge's tokens are in `apps/forge/apps/web/src/styles.css`. Layer's are in `apps/layer/src/theme.css`. Caster's icon copies Layer's colors in `apps/caster/Resources/AppIcon.svg`.

The shared palette gets written here first. `design/tokens` then carries the numbers, and each app updates its own colors from that. Until then, neither accent is the studio color.
