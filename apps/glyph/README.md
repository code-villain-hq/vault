# Glyph

*A Code Villain product.*

A local-first AI translation tool for managing i18n JSON files with LLM-powered auto-translation and confidence scoring.

## Features

- 🌍 Load and manage multiple translation JSON files (en.json, sv.json, etc.)
- 🤖 Auto-translate missing keys using local LLMs via llama-server
- 📊 Confidence scoring with low-confidence review workflow
- ⚙️ Configure LLM models and language pairs via web UI
- 📈 Dashboard with coverage metrics and statistics
- 🔧 Local-first design with optional remote LLM endpoints
- 🛡️ Never overwrites existing translations

## Tech Stack

- **Backend**: Go + Gin HTTP server
- **LLM**: llama-server (local or remote)
- **Frontend**: HTML/CSS/JavaScript (no build required)
- **Target OS**: macOS/Linux (WSL2 for Windows)

## Quick Start

### Prerequisites

- Go 1.21+
- llama-server (for LLM inference)
- macOS/Linux (WSL2 supported)

### Installation

1. Clone the repository:
```bash
git clone <repository-url>
cd glyph
```

2. Setup project:
```bash
make setup
```

3. Install dependencies:
```bash
make install-deps
```

4. Build the backend:
```bash
make build
```

### Running

Start the application:
```bash
make run
```

The app will be available at `http://localhost:3000`

### Development Mode

With auto-reload (requires `air`):
```bash
make dev
```

## Configuration

### Initial Setup

1. Copy the example config:
```bash
cp config.example.yaml config.yaml
```

2. Edit `config.yaml` to configure:
   - LLM endpoints
   - Language pairs
   - User preferences
   - Default models

### Via Web UI

Navigate to **Settings** tab to update:
- Source language (default: English)
- Default target language
- Confidence threshold
- LLM model mappings

Settings are saved to `~/.glyph/config.yaml`

## Workflow

### Dashboard View
- See overview of all language pairs
- View coverage percentage, missing count, and low-confidence count
- Click on a language pair to start translating

### Translate View
1. Select target language
2. Click **Translate Missing** to translate missing keys for single language
3. Or click **Translate All Languages** for bulk translation
4. Review low-confidence translations in the list
5. Click **Edit** to review and save manual corrections

### Review Low-Confidence Translations
1. Click on a low-confidence item
2. Edit the translation in the modal
3. Click **Save** to update

## Project Structure

```
glyph/
├── backend/
│   ├── main.go                 # Entry point
│   ├── internal/
│   │   ├── config/             # Configuration management
│   │   ├── translation/        # Translation file management
│   │   ├── llm/                # LLM client
│   │   └── routes/             # API endpoints
├── ui/
│   ├── index.html              # Web UI
│   ├── css/style.css           # Styling
│   └── js/                     # Frontend JavaScript
├── translations/               # Translation JSON files (en.json, sv.json, etc.)
├── docs/
│   ├── PLAN.md                 # Project plan
│   ├── API.md                  # API documentation
│   └── SETUP.md                # Detailed setup guide
├── config.yaml                 # Configuration (created on first run)
└── Makefile                    # Build commands
```

## API Endpoints

### Health & Info
- `GET /api/health` - Health check
- `GET /api/languages` - List available languages

### Configuration
- `GET /api/config` - Get configuration
- `POST /api/config` - Update configuration

### Translations
- `GET /api/translations/{lang}` - Get all translations
- `GET /api/translations/{lang}/missing` - Get missing keys
- `GET /api/translations/{lang}/low-confidence` - Get low-confidence items
- `POST /api/translate/missing` - Translate missing keys

### Metadata
- `GET /api/metadata/{lang}` - Get metadata
- `POST /api/metadata/{lang}/{key}` - Update metadata

See `docs/API.md` for detailed endpoint documentation.

## Setting Up llama-server

llama-server is used for running LLMs locally. Here's how to set it up:

### Installation

1. Download and install llama-server:
```bash
# macOS with Homebrew
brew install llama-cpp

# Or download from: https://github.com/ggerganov/llama.cpp
```

2. Download a model (e.g., Mistral):
```bash
llama-server -hf "mistral-community/Mistral-7B-Instruct-v0.1-GGUF" -ngl 35 -p "You are a translator."
```

### Running llama-server

```bash
# Start on default port 8000
llama-server -hf "mistral-community/Mistral-7B-Instruct-v0.1-GGUF" -ngl 35

# Or on a specific port
llama-server -p 8001 -nf "mistral-community/Mistral-7B-Instruct-v0.1-GGUF" -ngl 35
```

Then configure the endpoint in the app settings.

## Translation File Format

Translation files should be standard i18n JSON files:

```json
{
  "greeting": "Hello",
  "farewell": "Goodbye",
  "welcome": "Welcome to our app"
}
```

Files should be named with language codes: `en.json`, `sv.json`, `fi.json`, etc.

## Metadata Storage

Translation confidence and metadata are stored in `translations.meta.json`:

```json
{
  "translations": {
    "sv": {
      "greeting": {
        "confidence": 0.95,
        "translated_by": "mistral",
        "timestamp": "2024-04-18T12:00:00Z"
      }
    }
  }
}
```

## Examples

### Create Example Translation Files

```bash
make create-examples
```

This creates `translations/en.json` and `translations/sv.json` with sample data.

### API Usage Example

```bash
# Get health check
curl http://localhost:3000/api/health

# Get available languages
curl http://localhost:3000/api/languages

# Get missing translations for Swedish
curl http://localhost:3000/api/translations/sv/missing?source=en

# Translate missing keys
curl -X POST http://localhost:3000/api/translate/missing \
  -H "Content-Type: application/json" \
  -d '{"target_langs": ["sv"], "source_lang": "en"}'
```

## Troubleshooting

### Port already in use
Change the port in `config.yaml`:
```yaml
app:
  port: 3001
```

### LLM connection error
Ensure llama-server is running and the endpoint matches your configuration:
```bash
# Check if llama-server is running
curl http://localhost:8000/health
```

### Empty dashboard
1. Create translation files in the `translations/` directory
2. Restart the application
3. Check that file names match the pattern: `{lang}.json`

## Contributing

Feel free to submit issues and enhancement requests!

## License

MIT License - see LICENSE file for details

## Support

For help, feedback, or to report issues:
- Check `docs/` for detailed documentation
- See `docs/SETUP.md` for detailed setup instructions
- Report issues at: https://github.com/anomalyco/opencode

## Roadmap

- [ ] Auto-start llama-server management
- [ ] Export translations to other formats (YAML, TOML, etc.)
- [ ] Batch operations and workflows
- [ ] Translation memory and history
- [ ] Sync translations to remote repositories
- [ ] Docker support
