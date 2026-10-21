# Blog

The site's blog: an index of posts and the post page. It follows [`../principles.md`](../principles.md) and uses the studio type roles from [`brand/type.md`](../../brand/type.md). The site implements it in `apps/site/templates/`, `apps/site/blog.py` and `apps/site/site.css`; `build.py` renders the pages into `apps/site/blog/`.

## Header and footer

- The Code Villain wordmark at 10.5rem wide on the left: the page's one brand moment, so nothing else on the page uses poison as decoration.
- Navigation on the right in the `label` role: `muted`, the current page `fg` and underlined.
- The footer is one quiet line in `small`, `faint`: what Code Villain does, and the same two links. The links follow the link rule: ink, bold, underlined.
- No lines above or below either.

## Index

- The page title in `display` (Grenze), with one line under it in `muted`.
- Posts as rows, newest first, with no dividers. Each row: the date in `small`, `faint`, tabular; the title in `title`; one sentence of excerpt in `muted`; tags on the right.
- Tags are neutral chips (the product). A draft carries a `warn` status pill.
- A whole row is the link; hover fills `hover` with `radius-lg`.
- Under 640px the row stacks: date, title, excerpt, tags.

## Post

- A meta line in `small`, `faint`: "The log" (link back), product, date, reading time, and a draft pill when it is one.
- The title in `display`. Section headings in `headline`. Both Grenze, both sentence case.
- Body in `body`, 17px, at a 66ch measure.
- Links are ink, bold, underlined at 45%. Inline code in mono on `surface` with `radius-sm`.
- Images sit in a figure: full measure width, `radius-lg`, a caption under them in `small`, `faint`. Images with transparent grounds (the mark, a logo) sit on a `panel` block. No borders.
- Code blocks sit on `panel` with `radius-lg`: the language as a small label in `ui-small` 600, then mono in the `code` role. It takes the code palette (`design/tokens/syntax.css`, rules in [`theme.md`](../tokens/theme.md#code)), the same colours as the editors. Never poison. Long lines scroll inside the block.
- Ends with a quiet link back to all posts.

## Content

Posts are Markdown in [`content/blog`](../../content/blog), named `YYYY-MM-DD-slug.md`, with `title`, `date`, `product`, `summary`, and `draft` in front matter. The site's build renders them into these pages.
