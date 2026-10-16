# Blog

The site's blog: an index of posts and the post page. It follows [`../principles.md`](../principles.md) and uses the studio type roles from [`brand/type.md`](../../brand/type.md). The site implements it in `apps/site/blog/` and `apps/site/site.css`.

## Header and footer

- The logo at 7rem wide on the left: the page's one brand moment, so nothing else on the page uses poison as decoration.
- Navigation on the right in the `label` role: `muted`, the current page `fg` and underlined.
- The footer is one quiet line in `small`, `faint`: what Villain does, and the same two links.
- No lines above or below either.

## Index

- The page title in `display` (Grenze), with one line under it in `muted`.
- Posts as rows, newest first, with no dividers. Each row: the date in `small`, `faint`, tabular; the title in `title`; one sentence of excerpt in `muted`; tags on the right.
- Tags are neutral chips (the product). A draft carries a `warn` status pill.
- A whole row is the link; hover fills `hover` with `radius-lg`.
- Under 640px the row stacks: date, title, excerpt, tags.

## Post

- A meta line in `small`, `faint`: "The build" (link back), product, date, reading time, and a draft pill when it is one.
- The title in `display`. Section headings in `headline`. Both Grenze, both sentence case.
- Body in `body`, 17px, at a 66ch measure.
- Links are ink, bold, underlined at 45%. Inline code in mono on `surface` with `radius-sm`.
- Ends with a quiet link back to all posts.

## Content

Posts are Markdown in [`content/blog`](../../content/blog), named `YYYY-MM-DD-slug.md`, with `title`, `date`, `product`, and `draft` in front matter. Until the site has a build step, each post page is written by hand from its Markdown.
