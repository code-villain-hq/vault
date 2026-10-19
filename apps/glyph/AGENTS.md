# AGENTS.md

## Project Overview

**Glyph** (code name `glyph`) is a local-first AI translation tool for managing i18n JSON files. Backend: Go 1.21 + Gin. Frontend: vanilla HTML/CSS/JS (no build). Requires `llama-server` for LLM inference.

The UI follows the vault's `design/`: read `../../design/principles.md` before changing it. `ui/design/` is a copy of the Nightshade tokens, `components.css` and the self-hosted fonts; `make design` refreshes it from the vault and `make design-check` (CI) fails when it drifts. Tailwind (Play CDN) is there for layout only: `ui/js/theme.js` swaps its palette, type scale and radii for the token names (`bg-panel`, `text-muted`, `text-danger-ink`, `text-ui-small`), so palette classes like `bg-slate-800` generate nothing, and `design/lint` rejects them. Colours, sizes and radii in `ui/css/*.css` come from tokens. Every page shares the app frame (`ui/js/frame.js`: sidebar with the views, breadcrumb top bar, content panel) and Lucide icons from `ui/js/icons.js` (`icon('copy')`), not emoji.

## Build & Run

### Essential Commands

```bash
make setup              # One-time: create dirs, copy config.example.yaml → config.yaml
make install-deps      # Go mod tidy in ./backend
make build             # Builds to ./glyph (CGO_ENABLED=1)
make run               # build + ./glyph
make dev               # Auto-reload with air (installs if missing)
make clean             # Remove build artifacts
```

### Key Build Details

- **No CGO required** — pure-Go SQLite driver (`modernc.org/sqlite`)
- Binary output: `./glyph` (root, not backend/)
- UI served from `./ui/` directory (must exist at runtime)
- Config search order: `./config.yaml`, then `~/.glyph/config.yaml`
- DB-persisted config (`user_preferences`, `language_pairs`) overlays YAML on startup

## Configuration

### File Locations
- **Primary**: `./config.yaml` (created by `make setup` from `config.example.yaml`)
- **User-level**: `~/.glyph/config.yaml` (fallback)
- **DB overlay**: `config_kv` table in `./glyph.db` — mutable sections written here win over YAML

### Required Fields
```yaml
app:
  port: 3000
  translations_dir: ./translations
  db_path: ./glyph.db    # SQLite file; source of truth for translations + editable config

user_preferences:
  source_language: en
  default_target_language: sv

llm:
  models:
    - name: viking
      endpoint: http://localhost:8080   # llama-server endpoint (health check at /health)
      model: viking-7b-v4
      model_path: ./models/Viking_7B_V.4.Q4_K_M.gguf  # optional; enables Start in Settings UI
      # extra_args: ["-c", "4096"]                    # optional; appended to llama-server argv

language_pairs:
  en_sv:
    model: viking
    confidence_threshold: 0.7
```

## Storage Model

### Authoritative Source: SQLite

All translations, metadata and editable config live in `./glyph.db`. Tables:

- `translations(lang, namespace, key, value, updated_at)`
- `metadata(lang, namespace, key, confidence, translated_by, timestamp)`
- `config_kv(key, value_json, updated_at)`
- `schema_version(version)` — drives forward-only migrations

### Namespaces (locize/i18next conventions)

Every key lives inside a **namespace**. Common patterns:

- `common` — shared actions, greetings, time
- `auth`   — login/signup flows
- `dashboard`, `billing`, `settings`, …

If no namespace is specified, the API defaults to `translation` (matching locize/i18next defaults). Pass `?ns=<namespace>` to every `/api/translations/*` endpoint to target a specific one.

### JSON Export

Writes to the DB are mirrored out to JSON so downstream i18n tooling (e.g. `react-i18next`, `next-intl`) keeps working. Layout:

```
translations/
  en/
    common.json
    auth.json
    dashboard.json
  sv/
    common.json
    auth.json
  fi/
    common.json
  translations.meta.json   # nested lang → namespace → key → {confidence, translated_by, timestamp}
```

Legacy flat files (`translations/{lang}.json`) are still honored on first-run bootstrap: they're imported into the default namespace (`translation`).

### Bootstrap

If the DB is empty (fresh install), the Manager seeds it from whatever JSON it finds under `translations_dir`. Once the DB has at least one row, it becomes authoritative and JSON files are just a write-through cache.

## Backend Architecture

### Entry Point
`backend/main.go` initializes config, opens the SQLite DB, builds the translation manager, applies the config overlay, and registers the Gin router.

### Key Packages
- `internal/config/` — Viper YAML loader + DB overlay (`ApplyOverlay`, `PersistOverlay`)
- `internal/db/`      — SQLite driver wrapper + migrations + CRUD
- `internal/translation/` — In-memory cache, JSON import/export, read/write API
- `internal/llm/`     — HTTP client for OpenAI-compatible completion servers
- `internal/llmproc/` — Process manager for LLM backends the Settings UI starts/stops
- `internal/routes/`  — Gin handlers (all translation endpoints accept `?ns=`)

### API Routes
```
GET  /api/health
GET    /api/languages                                  # [{code, translation_count, namespaces, is_source}]
POST   /api/languages                                  # body: {code, model?} — creates a {source}_{code} pair so the language appears everywhere
DELETE /api/languages/{lang}                           # full removal: translations + metadata + FS + glossary + matching language_pairs + default_target fallback
GET    /api/namespaces?lang=xx                         # distinct namespaces; optional lang scope
GET    /api/namespaces/stats                           # [{namespace, key_count, translation_count}] summed across every language; drives Manage-namespaces modal
PUT    /api/namespaces/{ns}                            # body: {to} — rename every key across every language; refuses to merge into a populated target
DELETE /api/namespaces/{ns}                            # wipe every key in the namespace + its on-disk JSON files; metadata is re-exported
GET/POST /api/config

GET    /api/translations/{lang}?ns=common              # all key→value pairs for (lang, ns)
GET    /api/translations/{lang}/stats?source=en&ns=common
GET    /api/translations/{lang}/missing?source=en&ns=common
GET    /api/translations/{lang}/low-confidence?ns=common&threshold=0.7
PUT    /api/translations/{lang}/{key}?ns=common        # body: {translation, metadata?}
DELETE /api/keys?ns=common&key=login.submit            # language-agnostic: wipes key from every lang (translations + metadata) and re-exports affected JSON
DELETE /api/translations/{lang}                        # legacy alias for DELETE /api/languages/{lang}

POST /api/translate/single                           # body: {key, namespace, target_lang, source_lang, source_text?, dry_run?}
                                                      #   dry_run=true → return proposal without saving (used by Review's confirm flow)
                                                      #   source_text → override DB lookup; used by the Translations page's sparkles back-fill
                                                      #     button to translate an existing target value *into* the source language
                                                      #     when the source cell is empty.
POST /api/translate/missing                          # body: {target_langs, source_lang, namespace}

GET    /api/glossary?source_lang=en&target_lang=sv    # list glossary entries
POST   /api/glossary                                  # add  (body: {source_lang, target_lang, source_term, target_term, target_variants?, note?})
PUT    /api/glossary/{id}                             # update source_term, target_term, target_variants, note (pair is immutable)
DELETE /api/glossary/{id}                             # remove one entry
POST   /api/glossary/suggest-variants                 # body: {source_lang, target_lang, source_term, target_term} — ask the pair's LLM for grammatical inflections of target_term; returns {variants: [...]}. Backs the "Find variants" button in the Glossary editor.

GET  /api/metadata/{lang}?ns=common
PUT  /api/metadata/{lang}/{key}?ns=common
POST /api/import/preview                             # multipart: file, namespace (default for unprefixed keys). Parses JSON only — **no DB writes**. Returns `{count, default_namespace, namespaces[], sample_rows[]}` for the Import page review step (same locize `ns:key` rules as POST /import).
POST /api/import-preview                             # same handler as `/import/preview` (path alias for strict proxies).
POST /api/import                                     # multipart: file, lang, namespace, mode. Understands locize-style "ns:key" delimiters: keys like "billing:invoiceDueSoon" are routed to namespace "billing" regardless of the form's `namespace`, which is used only as the fallback for keys without a prefix. URL-shaped keys (anything with `://`) are left intact. The response includes a per-namespace breakdown: `{count, namespaces: [{namespace, count}, ...]}`.
GET  /api/search?q=...&lang=xx                       # searches every namespace
GET  /api/llm/test?endpoint=...
GET  /api/llm/status?name=viking                     # {reachable, managed, pid, can_start, ...}
POST /api/llm/start                                  # body: {name} — spawns `llm_binary` for that model
POST /api/llm/stop                                   # body: {name} — SIGTERMs a managed process
```

### UI Routes
- `/` → `ui/index.html` (dashboard with per-namespace coverage)
- `ui/translations.html` — browse/edit, filter by language + namespace. The "New key" button (⌘N) opens a modal that PUTs a new (namespace, key) for the source language via `/api/translations/:sourceLang/:key?ns=…`; creating a new namespace just writes it — the backend creates `translations/<lang>/<namespace>.json` on first save. The namespace dropdown also has a "New namespace…" entry that re-uses the same modal in "create namespace" mode (ns dropdown pre-set to new, input focused), plus a "Manage namespaces…" entry that opens a modal listing every namespace with its key/translation counts; rows have inline Rename (`PUT /api/namespaces/:ns`) and Delete (`DELETE /api/namespaces/:ns`) actions. Each key card has a copy icon right after the key name (writes the bare key — not `ns.key` — to the clipboard so developers can paste it straight into `t('…')` calls) and a trash icon in its header that calls `DELETE /api/keys?ns=…&key=…` — this wipes the key from every language (translations + metadata) and re-exports each affected JSON file. The copy action prefers `navigator.clipboard.writeText` on secure contexts and falls back to an invisible `<textarea>` + `execCommand('copy')` for older browsers / embedded webviews.
- `ui/missing.html` — review missing + low-confidence, filter by namespace
- `ui/glossary.html` — manage preferred-term rules (source_term → target_term per pair)
- `ui/settings.html` — Languages panel (one-click add/remove), LLM config, language pairs, keyboard-shortcuts legend

Night or day follows the system (`prefers-color-scheme`); there is no toggle. Source language, default target, and per-pair confidence threshold are edited via `config.yaml` / the Language Pairs table, not a General form.

Keyboard shortcuts are consistent across every page: `⌘1` Dashboard, `⌘2` Translations, `⌘3` Review, `⌘4` Glossary, `⌘5` Settings. On the Translations page, `⌘F` focuses search and `⌘N` opens the "add new key" modal.

## LLM Integration

### Recommended model: Viking 7B v.4 (Nordic languages)
```bash
make model-viking                                    # downloads to ./models/
llama-server -m ./models/Viking_7B_V.4.Q4_K_M.gguf -c 4096 --port 8080
```
Then set `llm.models[0].endpoint: http://localhost:8080` in `config.yaml` (default in the template).

### Any OpenAI-compatible server works
Health check at `GET {endpoint}/health`. Configure endpoint in UI settings or `config.yaml`.

### Starting/stopping servers from the UI
The Settings page shows a status pill (ok = reachable, warn = starting, neutral = stopped) plus Start / Stop buttons per configured LLM. Start requires `llm.models[].model_path` in `config.yaml` so the backend knows which GGUF to load; the port is taken from the configured `endpoint`. Externally-started servers still show as "Running (external)" but cannot be stopped via the UI. Managed processes are torn down automatically on app shutdown.

### Glossary (preferred-term rules)
Each row in the `glossary` table is `(source_lang, target_lang, source_term, target_term, target_variants, note)` with case-insensitive unique constraint on the source term within a pair. Before every LLM call, `Manager.GlossaryMatchesForText` filters the glossary to entries whose `source_term` appears (case-insensitive substring) in the text being translated, and passes them to `llm.Client.TranslateWithGlossary` which renders a `Glossary — MANDATORY preferred terms` block in the prompt. Notes are reviewer-only context and are NOT sent to the LLM. Manage entries on the dedicated **Glossary** page (`ui/glossary.html`, ⌘4).

`target_variants` is an optional list of additional accepted forms of `target_term`, used by both the prompt (so the LLM knows which inflections are OK) and the post-translation validator (so grammatically required forms like Swedish definite `e-posten` for canonical `e-post` aren't flagged as violations). Compounds are still rejected because the validator requires each accepted form to appear as an isolated word — `e-postadressen` never matches `e-post` regardless of variants. Empty variants ⇒ strict exact-word matching, identical to pre-v4 behaviour.

The Glossary editor exposes a **"Find variants"** button above the chip input in both the Add form and every row's edit mode. It POSTs to `/api/glossary/suggest-variants`, which routes through `TranslationService.SuggestGlossaryVariants` → `llm.Client.SuggestVariants` at temperature 0.1 with a prompt explicitly forbidding compounds and synonyms. The model's suggestions are merged case-insensitively into whatever the reviewer has already typed, so running it twice is idempotent. An empty list is a legitimate answer ("language has no inflection / model had nothing to add") and surfaces as an info toast rather than an error.

## Common Operations

### First Run
```bash
make setup
make seed-demo     # optional: reset DB + load the fake demo data (an existing translations/ is moved aside)
make run           # then, in another terminal: make seed-glossary
# Visit http://localhost:3000
```

### Add a New Namespace
Create `translations/{lang}/{namespace}.json` or call `POST /api/import` with `namespace={ns}`. The namespace appears automatically in the UI dropdowns once it contains at least one key.

### Add a New Language
1. Create `translations/{lang}/{ns}.json` for each namespace you care about.
2. Restart the app (or delete the DB and restart to force a full rebootstrap).
3. API: `POST /api/translate/missing` with `{target_langs: ["{lang}"], namespace: "common"}` to auto-fill.

### Troubleshooting

**Port 3000 in use**: Edit `config.yaml` → `app.port: 3001`, or `lsof -nP -iTCP:3000 -sTCP:LISTEN -t | xargs kill`.

**Empty dashboard**: hit `make seed-demo` then `make run`, or import a JSON file from **Import** (`/ui/import.html` — configure → review → merge).

**LLM connection error**: `curl http://localhost:8080/health` and check `llm.models[0].endpoint` matches the running server's port.

**Schema / data stuck**: `make reset-db && make run` re-bootstraps the DB from the JSON files on disk.

## Testing & Validation

No automated tests. Validation is manual via UI or curl. See `docs/API.md` for endpoint examples.

## Known Constraints

- **No automated tests** — changes require manual UI/API verification
- **UI assets required at runtime** — `./ui/` directory must sit next to the binary
- **Single llama-server endpoint** — app talks to one LLM at a time
- **JSON export is write-through only** — we don't garbage-collect JSON files when a namespace is fully removed; delete them by hand or `make reset-db`
