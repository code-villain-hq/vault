# Setup Guide - Detailed Instructions

## System Requirements

- **OS**: macOS 10.15+, Linux (Ubuntu 20.04+, Fedora 32+, Debian 11+), or WSL2
- **Go**: 1.21 or later
- **RAM**: 8GB minimum (16GB+ recommended for LLMs)
- **Disk Space**: 10GB+ for models

## Step 1: Install Go

### macOS
```bash
brew install go
```

### Linux (Ubuntu/Debian)
```bash
sudo apt-get update
sudo apt-get install golang-go
```

### Linux (Fedora)
```bash
sudo dnf install golang
```

Verify installation:
```bash
go version
```

## Step 2: Clone Repository

```bash
git clone <repository-url>
cd glyph
```

## Step 3: Project Setup

Initialize the project structure:

```bash
make setup
```

This creates:
- `translations/` directory for translation files
- `config.yaml` from the example template

## Step 4: Install Dependencies

```bash
make install-deps
```

This downloads Go dependencies including:
- Gin (HTTP framework)
- Viper (Configuration management)

## Step 5: Build the Backend

```bash
make build
```

Creates the `glyph` executable.

## Step 6: Setup llama-server

llama-server runs the LLM models locally.

### Installation

#### macOS with Homebrew
```bash
brew install llama-cpp
```

#### From Source
```bash
git clone https://github.com/ggerganov/llama.cpp
cd llama.cpp
make
```

#### Pre-compiled Binaries
Download from: https://github.com/ggerganov/llama.cpp/releases

### Download a Model

Models are downloaded from Hugging Face. Popular options:

**Option 1: Mistral 7B Instruct (Recommended)**
```bash
llama-server -hf "mistral-community/Mistral-7B-Instruct-v0.1-GGUF" \
  -ngl 35 \
  -c 2048 \
  -t 8
```

**Option 2: Llama 2 Chat**
```bash
llama-server -hf "TheBloke/Llama-2-7B-Chat-GGUF" \
  -ngl 35 \
  -c 2048 \
  -t 8
```

### Parameters Explanation
- `-hf` - Download from Hugging Face
- `-ngl 35` - Use GPU acceleration (35 layers on GPU)
- `-c 2048` - Context size
- `-t 8` - Number of threads

## Step 7: Configure the Application

### Edit config.yaml

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
    - name: mistral
      endpoint: http://localhost:8000
      model: mistral-7b

language_pairs:
  en_sv:
    model: mistral
    confidence_threshold: 0.7
```

### Or Configure via Web UI

1. Start the app: `make run`
2. Go to `http://localhost:3000`
3. Click **Settings** tab
4. Update preferences and save

## Step 8: Create Translation Files

Create translation JSON files in the `translations/` directory:

**translations/en.json:**
```json
{
  "greeting": "Hello",
  "farewell": "Goodbye",
  "welcome": "Welcome"
}
```

**translations/sv.json:**
```json
{
  "greeting": "Hej",
  "farewell": "Hej då"
}
```

Or use the example generator:
```bash
make create-examples
```

## Step 9: Start the Application

### Terminal 1 - Start llama-server
```bash
llama-server -hf "mistral-community/Mistral-7B-Instruct-v0.1-GGUF" -ngl 35
```

### Terminal 2 - Start the App
```bash
make run
```

The app is now available at: **http://localhost:3000**

## Step 10: First Translation

1. Open browser to `http://localhost:3000`
2. View the Dashboard - should show language pairs
3. Click on a language pair (e.g., EN→SV)
4. Go to Translate tab
5. Select target language
6. Click "Translate Missing"
7. Wait for LLM to process
8. Review low-confidence translations

## Troubleshooting

### "Connection refused" when starting app

Check if llama-server is running:
```bash
curl http://localhost:8000/health
```

If not running, start llama-server first in a separate terminal.

### "Port 3000 already in use"

Change the port in `config.yaml`:
```yaml
app:
  port: 3001
```

Or kill the existing process:
```bash
lsof -ti:3000 | xargs kill -9
```

### "Language code not found"

Ensure translation files are named correctly:
- `en.json` for English
- `sv.json` for Swedish
- `fi.json` for Finnish
- etc.

Files must be in the `translations/` directory.

### Translation results are empty

1. Verify English translation file has content
2. Check LLM endpoint is correct in config.yaml
3. Check llama-server is running and accessible
4. Look at browser console for error messages

### Model download is slow

Large models (7B+) can take 10-30 minutes to download depending on internet speed. This is normal.

To speed up:
- Use a smaller model: Mistral 7B Instruct (4GB) instead of larger models
- Pre-download the model before starting llama-server

### Out of memory errors

Reduce model size or GPU layers:
```bash
# Use CPU only (slower)
llama-server -hf "mistral-community/Mistral-7B-Instruct-v0.1-GGUF" -ngl 0

# Use fewer GPU layers
llama-server -hf "mistral-community/Mistral-7B-Instruct-v0.1-GGUF" -ngl 10
```

## Development Mode

For development with auto-reload:

1. Install `air` (auto-reload tool):
```bash
go install github.com/cosmtrek/air@latest
```

2. Start with auto-reload:
```bash
make dev
```

Changes to Go files will automatically rebuild and restart the server.

## Next Steps

- Check [API.md](API.md) for API documentation
- Read [PLAN.md](PLAN.md) for architecture details
- Explore the web UI at `http://localhost:3000`
- Create translation files for your project
- Configure language-specific LLM models

## Common Commands

```bash
# Setup project
make setup

# Install dependencies
make install-deps

# Build backend
make build

# Run application
make run

# Development mode (with auto-reload)
make dev

# Clean build artifacts
make clean

# Create example translation files
make create-examples
```

## Additional Resources

- [Llama.cpp Documentation](https://github.com/ggerganov/llama.cpp)
- [Hugging Face Models](https://huggingface.co/models)
- [i18n JSON Format](https://www.w3.org/International/questions/qa-what-is-web-internationalization)
- [Gin Framework Docs](https://gin-gonic.com/)
