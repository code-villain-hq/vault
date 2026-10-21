# Blog

Posts live here, one Markdown file each, named `YYYY-MM-DD-slug.md`, with `title`, `date`, `product`, `summary`, and `draft` in front matter. The summary is one sentence: the excerpt on the index and the page's description. A post stays `draft: true` until it is ready.

The site in `code/apps/site` generates its blog from these files: `python3 build.py` there. Images go in [`images/`](images), linked relative to the post; an image's title becomes its caption. The layout is in [`studio/design/layouts/blog-post.md`](../../design/layouts/blog-post.md).
