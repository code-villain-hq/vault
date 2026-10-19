package main

import (
	"context"
	"fmt"
	"net/http"
	"os"
	"os/signal"
	"path/filepath"
	"syscall"
	"time"

	"github.com/gin-gonic/gin"
	"glyph/internal/config"
	"glyph/internal/db"
	"glyph/internal/llmproc"
	"glyph/internal/routes"
	"glyph/internal/translation"
)

func main() {
	cfg, err := config.Load()
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to load config: %v\n", err)
		os.Exit(1)
	}

	// Open the SQLite store. This is the persistent source of truth for
	// translations, metadata and the mutable config overlay.
	store, err := db.Open(cfg.App.DBPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to open database at %s: %v\n", cfg.App.DBPath, err)
		os.Exit(1)
	}
	defer store.Close()

	// Apply any persisted overlay (user_preferences, language_pairs) on top of
	// what YAML provided. This is what makes "edit in UI -> survives restart"
	// actually work.
	if err := config.ApplyOverlay(cfg, store); err != nil {
		fmt.Fprintf(os.Stderr, "Failed to apply config overlay: %v\n", err)
		os.Exit(1)
	}

	translationMgr, err := translation.NewManager(cfg.App.TranslationsDir, store)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to initialize translation manager: %v\n", err)
		os.Exit(1)
	}

	// Process manager for any LLM backends the UI chooses to spawn. We
	// shut it down alongside the HTTP server so child processes don't leak.
	llmProcs := llmproc.New()
	defer llmProcs.StopAll(5 * time.Second)

	router := gin.Default()

	// Static UI.
	router.Static("/ui", filepath.Join(".", "ui"))
	router.StaticFile("/", filepath.Join(".", "ui", "index.html"))

	apiGroup := router.Group("/api")
	{
		apiGroup.GET("/health", routes.Health())

		apiGroup.GET("/languages", routes.GetLanguages(translationMgr, cfg))
		apiGroup.POST("/languages", routes.AddLanguage(cfg, store))
		apiGroup.DELETE("/languages/:lang", routes.DeleteLanguage(translationMgr, cfg, store))
		apiGroup.GET("/namespaces", routes.GetNamespaces(translationMgr))
		// Namespace management: stats view (key + translation counts),
		// rename (re-bucket every key across every language), and delete
		// (wipe every key in the namespace + its on-disk JSON files).
		apiGroup.GET("/namespaces/stats", routes.GetNamespaceStats(translationMgr))
		apiGroup.PUT("/namespaces/:ns", routes.RenameNamespace(translationMgr))
		apiGroup.DELETE("/namespaces/:ns", routes.DeleteNamespace(translationMgr))

		apiGroup.GET("/config", routes.GetConfig(cfg))
		apiGroup.POST("/config", routes.UpdateConfig(cfg, store))

		apiGroup.GET("/translations/:lang", routes.GetTranslations(translationMgr))
		apiGroup.GET("/translations/:lang/stats", routes.GetTranslationStats(translationMgr))
		apiGroup.GET("/translations/:lang/missing", routes.GetMissingTranslations(translationMgr))
		// /missing-source surfaces keys where the source lang itself is empty
		// — used by Review's "missing English" rows, which back-translate
		// from whatever other language has the key filled in.
		apiGroup.GET("/translations/:lang/missing-source", routes.GetMissingSource(translationMgr))
		apiGroup.GET("/translations/:lang/low-confidence", routes.GetLowConfidenceTranslations(translationMgr))
		apiGroup.PUT("/translations/:lang/:key", routes.UpdateTranslation(translationMgr))
		// Language-agnostic "delete this key everywhere" (query params
		// ?ns=X&key=Y). Separate path from /translations/:lang so it doesn't
		// collide with the legacy per-language DELETE.
		apiGroup.DELETE("/keys", routes.DeleteKey(translationMgr))
		// Legacy alias — `/translations/:lang DELETE` used to be the only way
		// to remove a language; keeping it so old UI builds keep working.
		apiGroup.DELETE("/translations/:lang", routes.DeleteLanguage(translationMgr, cfg, store))
		apiGroup.POST("/translate/single", routes.TranslateSingle(translationMgr, cfg))
		apiGroup.POST("/translate/missing", routes.TranslateMissing(translationMgr, cfg))

		apiGroup.GET("/metadata/:lang", routes.GetMetadata(translationMgr))
		apiGroup.PUT("/metadata/:lang/:key", routes.UpdateMetadata(translationMgr))

		apiGroup.POST("/import/preview", routes.PreviewImport())
		// Hyphen alias: some proxies mishandle multi-segment paths under /import/.
		apiGroup.POST("/import-preview", routes.PreviewImport())
		apiGroup.POST("/import", routes.ImportTranslations(translationMgr, cfg))

		apiGroup.GET("/search", routes.SearchTranslations(translationMgr))

		apiGroup.GET("/glossary", routes.ListGlossary(translationMgr))
		apiGroup.POST("/glossary", routes.AddGlossaryEntry(translationMgr))
		apiGroup.PUT("/glossary/:id", routes.UpdateGlossaryEntry(translationMgr))
		apiGroup.DELETE("/glossary/:id", routes.DeleteGlossaryEntry(translationMgr))
		// "Find variants": ask the pair's LLM for grammatical inflections of
		// a glossary target term so reviewers can populate target_variants
		// without typing every form by hand.
		apiGroup.POST("/glossary/suggest-variants", routes.SuggestGlossaryVariants(translationMgr, cfg))

		apiGroup.GET("/llm/test", routes.LLMTest(cfg))
		apiGroup.GET("/llm/status", routes.LLMStatus(cfg, llmProcs))
		apiGroup.POST("/llm/start", routes.LLMStart(cfg, llmProcs))
		apiGroup.POST("/llm/stop", routes.LLMStop(llmProcs))
	}

	port := cfg.App.Port
	srv := &http.Server{
		Addr:    fmt.Sprintf(":%d", port),
		Handler: router,
	}

	// Run the HTTP server in a goroutine so we can react to SIGINT/SIGTERM
	// and tear down managed LLM processes before exiting.
	go func() {
		fmt.Printf("Starting server on port %d (db: %s)...\n", port, cfg.App.DBPath)
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			fmt.Fprintf(os.Stderr, "Failed to start server: %v\n", err)
			os.Exit(1)
		}
	}()

	stop := make(chan os.Signal, 1)
	signal.Notify(stop, syscall.SIGINT, syscall.SIGTERM)
	<-stop
	fmt.Println("Shutting down...")

	shutdownCtx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	_ = srv.Shutdown(shutdownCtx)
	llmProcs.StopAll(5 * time.Second)
}
