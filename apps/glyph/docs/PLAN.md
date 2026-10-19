# AI Translation Tool - Project Plan

## Overview
A local-first AI translation tool that helps manage i18n JSON files. It loads translation files (en.json, sv.json, etc.), identifies missing translations, and uses LLMs to auto-translate them with confidence scoring.

**Tech Stack:**
- Backend: Go + GIN HTTP server
- LLM: llama-server (local or remote)
- Frontend: Plain HTML/CSS/JavaScript
- Target OS: macOS/Linux (WSL2 for Windows)

---

## Features

### 1. Translation Management
- Load i18n JSON files from `translations/` directory
- Each file named `{lang}.json` (en.json, sv.json, fi.json, etc.)
- Detect missing keys across languages
- Show coverage percentage per language pair
- Preserve already-translated strings (no overwrites)

### 2. User Preferences
- Store in `~/.glyph/config.json`
- Default source language: English (en)
- Default target language: Configurable (e.g., sv)
- Remember last viewed language pair

### 3. LLM Configuration
- Configure via web UI
- Map language pairs to LLM endpoints
- Example: `en→sv: http://localhost:8000`, `en→de: http://localhost:8001`
- LLM defines confidence thresholds
- Sane defaults provided

### 4. Translation Workflow
**Primary Flow:**
- Select target language pair (source: English by default)
- View missing untranslated keys
- Click "Translate Missing" button
  - Translate single target language, OR
  - Bulk translate to all configured target languages
- LLM returns translation + confidence score
- If confidence is low, add to review queue
- Dashboard shows metrics:
  - Total keys
  - Missing translations
  - Low-confidence translations
  - Coverage %

**Review Flow:**
- List low-confidence translations
- Click to view/edit
- Save updated translation
- Update metadata

### 5. llama-server Management
- Auto-start on app launch (if configured)
- Auto-stop on app quit
- Configurable models per language pair
- Models downloaded via `llama-server -hf` flag by default
- Default storage: `~/.glyph/models/`

### 6. Metadata Storage
- Store in `translations.meta.json` alongside translation files
- Structure:
  ```json
  {
    "translations": {
      "sv": {
        "key1": {"confidence": 0.95, "translated_by": "viking", "timestamp": "2024-04-18T12:00:00Z"},
        "key2": {"confidence": 0.65, "translated_by": "mistral", "timestamp": "2024-04-18T12:01:00Z"}
      }
    }
  }
  ```

---

## Project Structure

```
glyph/
├── docs/
│   ├── PLAN.md                    # This file
│   ├── API.md                     # API documentation
│   └── SETUP.md                   # Setup instructions
├── backend/
│   ├── main.go
│   ├── go.mod
│   ├── go.sum
│   └── internal/
│       ├── config/
│       │   ├── config.go          # Load config
│       │   └── defaults.go        # Default settings
│       ├── translation/
│       │   ├── loader.go          # Load JSON files
│       │   ├── manager.go         # Manage translations
│       │   └── metadata.go        # Manage metadata
│       ├── llm/
│       │   ├── client.go          # LLM API client
│       │   └── llamaserver.go     # llama-server manager
│       └── routes/
│           ├── health.go
│           ├── languages.go
│           ├── config.go
│           ├── translate.go
│           └── metadata.go
├── ui/
│   ├── index.html
│   ├── import.html
│   ├── css/
│   │   └── style.css
│   └── js/
│       ├── api.js
│       ├── utils.js
│       ├── dashboard.js
│       ├── import.js
│       ├── translations.js
│       ├── missing.js
│       ├── glossary.js
│       └── settings.js
├── config.example.yaml            # Example configuration
├── translations/                  # Translation files (user-created)
│   ├── en.json
│   └── sv.json
├── translations.meta.json         # Metadata (auto-created)
├── go.mod
├── go.sum
├── Makefile
└── README.md
```

---

## API Endpoints

### Health & Info
- `GET /api/health` - Health check
- `GET /api/languages` - List available languages with metadata

### Configuration
- `GET /api/config` - Get current configuration
- `POST /api/config` - Update configuration
- `GET /api/config/llm-models` - List available LLM models

### Translations
- `GET /api/translations/{lang}` - Get all translations for a language
- `GET /api/translations/{lang}/missing` - Get missing keys for target language
- `GET /api/translations/{lang}/low-confidence` - Get low-confidence translations
- `POST /api/translate/single` - Translate a single key
- `POST /api/translate/missing` - Translate all missing keys (single target or bulk)

### Metadata
- `GET /api/metadata/{lang}` - Get metadata for language
- `POST /api/metadata/{lang}/{key}` - Update metadata for a key

---

## Configuration (config.yaml)

```yaml
app:
  port: 3000
  translations_dir: ./translations
  models_dir: ~/.glyph/models

user_preferences:
  source_language: en
  default_target_language: sv

llm:
  models:
    - name: viking
      endpoint: http://localhost:8001
      model: viking-model
    - name: mistral
      endpoint: http://localhost:8002
      model: mistral-model

language_pairs:
  en_sv:
    model: viking
    confidence_threshold: 0.7
  en_fi:
    model: viking
    confidence_threshold: 0.7
```

---

## User Workflow

1. **Launch App**
   - Backend starts, UI opens in browser
   - Checks for llama-server, starts if configured

2. **Dashboard View**
   - Shows all available language pairs
   - Coverage %, missing count, low-confidence count per pair

3. **Translate Missing**
   - Select target language (or all)
   - Click "Translate Missing"
   - Backend:
     - Finds missing keys in target language
     - Calls LLM for each key
     - Stores translation + confidence in metadata
     - Returns results to UI
   - UI shows:
     - New translations added
     - Low-confidence items highlighted
     - Option to review now

4. **Review Low-Confidence**
   - Click on low-confidence item
   - See original English + AI translation + confidence score
   - Edit if needed
   - Save (updates both translation file and metadata)

5. **Export/Use**
   - Translation files ready for app use
   - Metadata preserved for future context

---

## Development Phases

### Phase 1: Foundation
- [x] Project structure
- [ ] Go backend scaffold with GIN
- [ ] Translation file loader
- [ ] Basic API endpoints (list languages, get translations)
- [ ] Configuration management

### Phase 2: Core Translation
- [ ] LLM client integration
- [ ] Single key translation endpoint
- [ ] Metadata storage and retrieval
- [ ] Missing keys detection

### Phase 3: Web UI
- [ ] Dashboard with language pair overview
- [ ] Translation viewer/editor
- [ ] Configuration UI
- [ ] Review/approval workflow

### Phase 4: Polish
- [ ] llama-server auto-management
- [ ] Error handling and logging
- [ ] Performance optimization
- [ ] Documentation and examples

---

## Key Design Decisions

1. **i18n JSON Format**: Standard flat key-value structure (can support nested with dot notation)
2. **Confidence**: LLM-determined, stored separately to avoid metadata in translation files
3. **Manual Trigger**: Translation only starts with explicit user action (safe by default)
4. **Local First**: All processing local, optional remote LLM endpoints
5. **No Overwrites**: Only adds missing translations, never modifies existing ones
6. **Config via UI**: Non-technical users can configure without editing YAML
