# Site

The Code Villain website at **https://www.codevillain.eu/**: the homepage, and the blog in [`blog/`](blog). Shared styles are in [`site.css`](site.css).

The blog is generated from [`content/blog`](../../content/blog). `python3 build.py` renders each post's Markdown into `blog/` through [`templates/`](templates) with [`blog.py`](blog.py): a page per post and the index, newest first. `blog/` is build output, not in git; edit the Markdown or the templates. The preview shows drafts with a pill; `dist/` leaves them out.

The Markdown is the plain subset the posts need: `##` and `###` headings, paragraphs, lists, quotes, fenced code, links, images, inline code, strong and emphasis. Straight quotes become curly. A paragraph of images alone is a figure, the image title (`![alt](path "caption")`) its caption; two images in one paragraph share a panel. Image paths are relative to the post. CSS and SVG/HTML code takes the code palette; other languages stay plain. Reading time counts prose at 200 words a minute.

It reads colour, type, and shape from [`design/tokens`](../../design/tokens) by relative path, so run `python3 build.py` once for the blog, then open it through the vault's preview server (`vault-static` in `.claude/launch.json`, then `/apps/site/`), not from disk. `python3 build.py` copies the public assets and rewrites their references for deployment at the domain root.

The homepage uses the outlined [`logo-home.svg`](../../brand/mark/logo-home.svg) variant through an external SVG reference. Its extended strike and cropped lettering are defined in [`brand/mark/README.md`](../../brand/mark/README.md).

Fonts are self-hosted in [`assets/fonts/`](assets/fonts/), including the reading italic and each family’s SIL license. Every page loads `assets/fonts/fonts.css`; the brand reference page reuses it. Ship this folder with the site.

The homepage keeps the extended-strike logo above a product showcase. Forge leads with a real capture of its own workspace; Layer and Caster sit beneath it, stacking below 800px, and Glyph closes the showcase at full width. The header menu links to Forge, Layer, Caster, Glyph, and The log; The log also has a quiet footer link. The introductory heading stands alone, without extra promotional copy on the right. Styles specific to the homepage live in [`home.css`](home.css).

Every product card is a screen capture from [`assets/captures/`](assets/captures/); `build.py` ships only the captures a page shows. Layer’s is its task view in Nightshade night, taken at 2× from its mock harness (`apps/layer`, `/mock.html?view=work&task=t-login`, the “pane not reopened” notice dismissed and the browser panel closed, since its white example page breaks the night palette), so its tasks and agents are the harness’s example data, and its caption says so. Caster’s card stacks three captures of its window alone (a sum, a currency, an app), on the card’s ground rather than a wallpaper. Glyph’s is its translations view in Nightshade night, taken at 2× from `bun run dev` with `bun run seed` (passwords on, so `GLYPH_OIDC_ISSUER` empty), one key of the `henchmen` namespace open; its strings are the made-up ones in `apps/glyph/demo`, and its caption says so. Product icons come from each app’s existing SVG asset. No live user data is included. The page follows “show less”, “tone, not lines”, and “display for headings” from the shared principles.

Page titles, descriptions, canonical links, and Open Graph metadata use Code Villain and the `www.codevillain.eu` origin. Serve the homepage at `/` and the blog at `/blog/`; keep the existing post URL when deploying. DNS, TLS, and redirects for the bare domain are hosting configuration.

## Build and host

Run `python3 build.py` from this folder. The `dist/` folder contains only the public HTML, styles, fonts and licenses, outlined marks, product icons, blog illustrations, and `.htaccess`. The builder checks every local HTML and CSS dependency. Vault source, internal design documents, app code, and brand drafts stay out of the upload. The `/design/` URL space returns 404.

`uberspace.htaccess` becomes the site's `.htaccess`. It sends the bare domain to `https://www.codevillain.eu/`, answers `/design/` with 404, and redirects old Forgejo paths that match no site file to `https://git.codevillain.eu/`, keeping Git HTTP request methods.

Where the site is hosted and how a build is deployed lives in the private `infra` repository, not here. Never upload a vault checkout into a public document root.
