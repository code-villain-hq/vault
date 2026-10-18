# Site

The Code Villain website at **https://www.codevillain.eu/**: the homepage, and the blog in [`blog/`](blog). Shared styles are in [`site.css`](site.css).

The blog shows posts from [`content/blog`](../../content/blog). Each post page is written by hand from its Markdown; `build.py` packages the finished pages without generating posts. The blog index lists the real post, currently marked as a draft.

It reads colour, type, and shape from [`design/tokens`](../../design/tokens) by relative path, so open it through the vault's preview server (`vault-static` in `.claude/launch.json`, then `/apps/site/`), not from disk. `python3 build.py` copies the public assets and rewrites their references for deployment at the domain root.

The homepage uses the outlined [`logo-home.svg`](../../brand/mark/logo-home.svg) variant through an external SVG reference. Its extended strike and cropped lettering are defined in [`brand/mark/README.md`](../../brand/mark/README.md).

Fonts are self-hosted in [`assets/fonts/`](assets/fonts/), including the reading italic and each family’s SIL license. Every page loads `assets/fonts/fonts.css`; the brand reference page reuses it. Ship this folder with the site.

The homepage keeps the extended-strike logo above a product showcase. Forge leads with a real capture of its own workspace; Layer and Caster sit beneath it, stacking below 800px. The header menu links to Forge, Layer, Caster, and The log; The log also has a quiet footer link. The introductory heading stands alone, without extra promotional copy on the right. Styles specific to the homepage live in [`home.css`](home.css).

Forge’s and Layer’s cards are screen captures from [`assets/captures/`](assets/captures/); `build.py` ships only the captures a page shows. Layer’s is its task view in Nightshade night, taken at 2× from its mock harness (`apps/layer`, `/mock.html?view=work&task=t-login`, the “pane not reopened” notice dismissed), so its tasks and agents are the harness’s example data, and its caption says so. The Caster illustration is HTML/CSS with explicitly labelled example content: its documented calculator and keyboard shortcut. Product icons come from each app’s existing SVG asset. No live user data is included. The page follows “show less”, “tone, not lines”, and “display for headings” from the shared principles.

Page titles, descriptions, canonical links, and Open Graph metadata use Code Villain and the `www.codevillain.eu` origin. Serve the homepage at `/` and the blog at `/blog/`; keep the existing post URL when deploying. DNS, TLS, and redirects for the bare domain are hosting configuration.

## Build and host

Run `python3 build.py` from this folder. The `dist/` folder contains only the public HTML, styles, fonts and licenses, outlined marks, product icons, blog illustrations, and `.htaccess`. The builder checks every local HTML and CSS dependency. Vault source, internal design documents, app code, and brand drafts stay out of the upload. The `/design/` URL space returns 404.

`uberspace.htaccess` becomes the site's `.htaccess`. It sends the bare domain to `https://www.codevillain.eu/`, answers `/design/` with 404, and redirects old Forgejo paths that match no site file to `https://git.codevillain.eu/`, keeping Git HTTP request methods.

Where the site is hosted and how a build is deployed lives in the private `infra` repository, not here. Never upload a vault checkout into a public document root.
