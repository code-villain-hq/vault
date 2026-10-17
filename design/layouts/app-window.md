# App window

The frame Forge and Layer share: a sidebar and a top bar that control the content. It follows [`../principles.md`](../principles.md) (chrome dims, content leads; tone, not lines) and uses the controls in [`../components.md`](../components.md).

## Regions

| Region | Tone | Holds |
| --- | --- | --- |
| Sidebar | `bg` | The app's name, then navigation. Full height, left. |
| Top bar | `bg` | Where you are, and tools for the whole view: a breadcrumb, a filter or search. |
| Content | `panel`, `radius-lg` | The view. Inset 0.5rem from the window's right and bottom edges, so the frame shows around it. |

The sidebar and top bar together form an inverted L. Nothing divides the regions: the content is one step lighter by night and white by day.

## Sidebar

- The fixed product wordmark at the top in Grenze Gotisch, 28px, weight 700. The only Grenze in the frame. It reads "Forge" in Forge regardless of the workspace name.
- Global Search and the product's create action (New issue in Forge) sit together at the right of the wordmark row, as quiet 28px icon buttons with accessible names. They do not occupy a separate row between the workspace and navigation.
- The workspace picker is a separate row below the product identity and above navigation. It shows the workspace's avatar and name; switching workspace never renames the product wordmark.
- Items in the `ui` role, text `muted`, 6px corners. Hover fills `hover` and turns the text `fg`.
- The current item fills `selected`, with text `fg` at 700.
- A count shows only when it asks for something: unread, waiting in Triage. The page you're on shows no count; its list says it. Empty counts never show.
- Unread counts carry a live dot in `accent`.
- Groups (a team, a project) get a small label in `faint`, `ui-small`. No dividers between groups.

## Top bar

- A breadcrumb in `ui-small`, `muted`, with the current place in `fg` at 600.
- View-wide tools sit on the right. A field here fills `panel`, a step off the frame.

## Content

- The top-bar breadcrumb names the view. Do not repeat that generic title as a large heading in the content: Inbox should not open with another visible "Inbox". Keep one accessible `h1` for the page, visually hidden when the toolbar already provides its visible name.
- A record's actual title remains visible in the `title` role (and editable where supported). Its identifier may appear in the breadcrumb. The accessible heading names the record, not only its identifier.
- Lists inset 0.5rem from the panel's edge. Rows have 6px corners and no dividers; hover and selection are tone steps.
- The view leads with one primary action at most. Everything else is a secondary button.

## Narrow windows

The current fallback under 560px turns the sidebar into a row of items across the top, without the app's name or group labels. The content keeps its 0.5rem inset on both sides. This is not a finished mobile layout; the mobile redesign is deferred. Live sync and help controls must remain visible outside the navigation scroller.

The [night-and-day page](https://claude.ai/artifact/GWrPzZDeYbkcRKiMW7E9gy) draws this frame in both modes.
