.PHONY: help build run dev clean setup install-deps seed-demo reset-db model-viking design design-check

# Viking 7B v.4 GGUF (Nordic languages specialist, good for sv/fi/da/no/is).
# Quantized Q4_K_M — ~4.3 GB, runs comfortably on a Mac with llama.cpp.
VIKING_MODEL_URL := https://huggingface.co/mradermacher/Viking_7B_V.4-GGUF/resolve/main/Viking_7B_V.4.Q4_K_M.gguf
VIKING_MODEL_FILE := models/Viking_7B_V.4.Q4_K_M.gguf

help:
	@echo "Translation Tool - Available Commands"
	@echo "======================================"
	@echo "make setup          - Setup project structure"
	@echo "make install-deps   - Install Go dependencies"
	@echo "make build          - Build the backend"
	@echo "make run            - Run the application"
	@echo "make dev            - Run in development mode with auto-reload"
	@echo "make clean          - Clean build artifacts"
	@echo "make seed-demo      - Reset DB and seed demo namespaces"
	@echo "make reset-db       - Delete the SQLite database"
	@echo "make model-viking   - Download Viking_7B_V.4 GGUF model"
	@echo "make design         - Copy the vault's tokens, controls and fonts into ui/design"
	@echo "make design-check   - Fail if ui/design has drifted from the vault"

setup:
	@mkdir -p translations
	@mkdir -p backend
	@cp config.example.yaml config.yaml 2>/dev/null || true
	@echo "✓ Project setup complete"

install-deps:
	@cd backend && go mod tidy
	@echo "✓ Dependencies installed"

build: install-deps
	@cd backend && go build -o ../glyph main.go
	@echo "✓ Build complete: glyph"

run: build
	@./glyph

dev:
	@which air > /dev/null || go install github.com/cosmtrek/air@latest
	@cd backend && air

clean:
	@rm -f glyph
	@cd backend && go clean
	@echo "✓ Clean complete"

# Stops the running server (if any), wipes the SQLite DB, and lets the next
# `make run` re-bootstrap from the seeded JSON namespaces under translations/.
reset-db:
	@PID=$$(lsof -nP -iTCP:3000 -sTCP:LISTEN -t 2>/dev/null); \
		[ -n "$$PID" ] && kill $$PID || true; \
		sleep 1
	@rm -f glyph.db glyph.db-shm glyph.db-wal
	@echo "✓ Database reset"

# Drop the DB and copy the committed demo namespaces from demo/ into translations/.
# After this, `make run` will bootstrap the DB from those JSON files.
seed-demo: reset-db
	@mkdir -p translations && cp -Rn demo/ translations/
	@echo "✓ Demo namespaces copied from demo/ into translations/ (existing files kept)"
	@echo "  Run 'make run' to bootstrap the DB from them."

# Download the Viking 7B GGUF. Use this with llama.cpp's llama-server:
#   llama-server -m $(VIKING_MODEL_FILE) -c 4096 --port 8080
# then point app.llm.models[0].endpoint at http://localhost:8080 in config.yaml.
model-viking:
	@mkdir -p models
	@if [ -f $(VIKING_MODEL_FILE) ]; then \
		echo "✓ Model already downloaded: $(VIKING_MODEL_FILE)"; \
	else \
		echo "→ Downloading Viking_7B_V.4 Q4_K_M GGUF (~4.3 GB)..."; \
		curl -L --fail -o $(VIKING_MODEL_FILE).tmp $(VIKING_MODEL_URL) && \
		mv $(VIKING_MODEL_FILE).tmp $(VIKING_MODEL_FILE) && \
		echo "✓ Saved to $(VIKING_MODEL_FILE)"; \
	fi
	@echo ""
	@echo "Next: run it with llama.cpp's llama-server:"
	@echo "  llama-server -m $(VIKING_MODEL_FILE) -c 4096 --port 8080"

# The UI keeps its own copy of the vault's design sources, so ./ui works next
# to the binary on its own: the Nightshade tokens, the shared ui-* controls and
# the licensed fonts. `make design` refreshes the copy; `make design-check`
# writes nothing and fails if it has drifted.
VAULT = ../..
DESIGN = ui/design
FONTS = fonts.css grenze-gotisch.ttf grenze-gotisch-OFL.txt \
	atkinson-hyperlegible-next.ttf atkinson-hyperlegible-next-italic.ttf atkinson-hyperlegible-next-OFL.txt \
	atkinson-hyperlegible-mono.ttf atkinson-hyperlegible-mono-OFL.txt
DESIGN_COPIES = \
	$(foreach f,color.css type.css shape.css,$(VAULT)/design/tokens/$(f):$(DESIGN)/$(f)) \
	$(VAULT)/design/components.css:$(DESIGN)/components.css \
	$(foreach f,$(FONTS),$(VAULT)/apps/site/assets/fonts/$(f):$(DESIGN)/fonts/$(f))

design:
	@mkdir -p $(DESIGN)/fonts
	@for pair in $(DESIGN_COPIES); do cp "$${pair%%:*}" "$${pair#*:}"; done
	@echo "✓ Synced tokens, controls and fonts into $(DESIGN)"

design-check:
	@stale=""; for pair in $(DESIGN_COPIES); do \
		cmp -s "$${pair%%:*}" "$${pair#*:}" || stale="$$stale $${pair#*:}"; \
	done; \
	if [ -n "$$stale" ]; then echo "Out of date with the vault:$$stale. Run make design."; exit 1; fi
