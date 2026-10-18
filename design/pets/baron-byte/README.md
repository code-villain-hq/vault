# Baron Byte

A hooded Code Villain schemer in silver armor and a violet cloak, drawn with Golden Age comic ink. Created and selected as the active ChatGPT pet on 18 October 2026.

The look follows [the brand palette](../../../brand/color.md) and [voice](../../../brand/voice.md): charcoal armor, a deep violet hood and cape, one poison-chartreuse clasp, and a theatrical, commanding expression. The user supplied the [hooded coder reference](sources/user-reference.png) and approved the final hooded character.

| Asset | Purpose |
| --- | --- |
| [baron-byte.png](baron-byte.png) | Approved full-size character with transparency |
| [spritesheet.png](spritesheet.png) | Validated pet artwork uploaded to ChatGPT |
| [animations/](animations/) | Nine state GIFs, all-state GIF and MP4, look loop, idle–jump–idle loop, and labeled stills |
| [prompts/](prompts/) | Generated base, animation, direction, and repair prompt templates |
| [sources/](sources/) | Approved source strips, user reference, layout guides, creation manifest, and registration/preview helpers |
| [qa/](qa/) | Contact sheets, direction reviews, independent blind checks, and validation reports |
| [pet.json](pet.json) | Stable pet ID, metadata, and activation snapshot |
| [checksums.json](checksums.json) | SHA-256 hashes verified against the copied source files |

The v2 sheet is a transparent 1536 × 2288 PNG: eight columns and eleven rows of 192 × 208 cells. It contains 57 animation frames and sixteen look directions; unused cells stay transparent. The nine states are idle, running right, running left, waving, jumping, failed, waiting, active work, and review.

Size revision, 18 October 2026: the character is fitted 28% larger within every cell. The idle silhouette is now 180 pixels tall, previously 141. The jump uses a shorter 13-pixel lift to keep the larger hood and boots inside the cell. The previous sheet, previews, metadata, and validation reports are preserved in [revisions/size-v1/](revisions/size-v1/). The active pet keeps the same stable ID.

This size revision resamples the existing approved pixels and adjusts registration; it introduces no new character art. [sources/enlarge_cells.py](sources/enlarge_cells.py) records the transformation. [qa/size-comparison.png](qa/size-comparison.png) shows the change at native cell size.

Artwork was generated with the built-in imagegen tool, then extracted, registered, assembled, and validated with the Pets skill scripts. Source prompts were augmented for transparent backgrounds, separated silhouettes, alternating gait, and head-direction repairs. The creation manifests and QA reports preserve original session paths as provenance. Helpers depend on the installed Pets skill scripts.

Final structural validation, quality checks, independent direction review, and Pets preflight passed. Stable pet ID: `pet_6ad47f0d72f0819184ecb45d33d10d1a`.
