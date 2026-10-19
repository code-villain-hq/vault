// Package config loads and persists Glyph configuration.
//
// Configuration lives in two places:
//
//  1. config.yaml — bootstrap values that have to be known *before* the DB can
//     be opened (port, translations_dir, models_dir, db_path, and the list of
//     llm.models endpoints). We keep using viper for this so users can
//     continue to hand-edit YAML.
//
//  2. SQLite  — user-editable overlays for the mutable sections
//     (user_preferences and language_pairs). These are saved to the DB on
//     every UI edit so they survive restarts without anyone having to write
//     YAML. LoadOverlay applies them on top of the YAML values.
//
// The split means: you can always hand-edit config.yaml and expect it to
// matter for bootstrap; everything else round-trips through the UI and the
// database.
package config

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/spf13/viper"
)

// Config is the fully-resolved configuration (YAML + DB overlay).
type Config struct {
	App             AppConfig               `mapstructure:"app"`
	UserPreferences UserPreferencesConfig   `mapstructure:"user_preferences"`
	LLM             LLMConfig               `mapstructure:"llm"`
	LanguagePairs   map[string]LanguagePair `mapstructure:"language_pairs"`
}

// AppConfig is bootstrap-only (YAML).
type AppConfig struct {
	Port            int    `mapstructure:"port" json:"port"`
	TranslationsDir string `mapstructure:"translations_dir" json:"translations_dir"`
	ModelsDir       string `mapstructure:"models_dir" json:"models_dir"`
	// DBPath is where SQLite stores persistent state. Resolved with env
	// expansion like the other paths. Default: ./glyph.db
	DBPath string `mapstructure:"db_path" json:"db_path"`
	// LLMBinary is the executable used to start managed LLM backends (e.g.
	// "llama-server" from llama.cpp). Looked up on $PATH unless an absolute
	// path is given. Default: "llama-server".
	LLMBinary string `mapstructure:"llm_binary" json:"llm_binary"`
}

// UserPreferencesConfig can be edited at runtime and is persisted to the DB.
type UserPreferencesConfig struct {
	SourceLanguage        string `mapstructure:"source_language" json:"source_language"`
	DefaultTargetLanguage string `mapstructure:"default_target_language" json:"default_target_language"`
}

// LLMConfig is currently bootstrap-only (YAML) — endpoints aren't editable
// from the UI yet.
type LLMConfig struct {
	Models []LLMModel `mapstructure:"models" json:"models"`
}

// LLMModel describes one configured llama-server-like backend.
type LLMModel struct {
	Name     string `mapstructure:"name" json:"name"`
	Endpoint string `mapstructure:"endpoint" json:"endpoint"`
	Model    string `mapstructure:"model" json:"model"`
	// ChatMode when true uses /v1/chat/completions instead of /completion.
	// This is now the DEFAULT for all models — llama-server supports it for
	// every model. Set to false only if your server doesn't support the chat
	// endpoint (non-llama-server setups that only expose /completion).
	ChatMode *bool `mapstructure:"chat_mode" json:"chat_mode,omitempty"`
	// ModelPath is the GGUF file path passed to the LLM binary via `-m`.
	// Use this OR (HFRepo + HFFile) for Hugging Face download mode — see
	// CanStart / LLMStart in routes.
	ModelPath string `mapstructure:"model_path" json:"model_path"`
	// HFRepo and HFFile enable `llama-server --hf-repo … --hf-file …` instead
	// of `-m <local.gguf>`. Both must be non-empty when using this mode.
	HFRepo string `mapstructure:"hf_repo" json:"hf_repo,omitempty"`
	HFFile string `mapstructure:"hf_file" json:"hf_file,omitempty"`
	// ExtraArgs are appended after the auto-generated argv (either -m/--port
	// or --hf-repo/--hf-file/--port).
	ExtraArgs []string `mapstructure:"extra_args" json:"extra_args"`
}

// CanStart reports whether the Settings UI is allowed to spawn this server:
// either a local GGUF path is set, or a Hugging Face repo+file pair is set.
func (m LLMModel) CanStart() bool {
	if strings.TrimSpace(m.HFRepo) != "" && strings.TrimSpace(m.HFFile) != "" {
		return true
	}
	return strings.TrimSpace(m.ModelPath) != ""
}

// HFDownloadMode is true when this entry should be started with --hf-repo /
// --hf-file instead of -m.
func (m LLMModel) HFDownloadMode() bool {
	return strings.TrimSpace(m.HFRepo) != "" && strings.TrimSpace(m.HFFile) != ""
}

// LanguagePair is the per-direction translation config, editable at runtime
// from the UI and persisted via the DB overlay. Each key is "{source}_{target}"
// (e.g. "en_sv", "sv_en") so both directions of a language pair can be tuned
// independently — some LLMs translate better one way than the other.
type LanguagePair struct {
	// Model is the `llm.models[].name` to use for this direction. If empty or
	// not found, the first configured model wins.
	Model string `mapstructure:"model" json:"model"`
	// Temperature passed to the LLM for this pair. 0.0–2.0, lower is more
	// deterministic. Defaults to 0.2 if unset/zero.
	Temperature float64 `mapstructure:"temperature" json:"temperature"`
	// ConfidenceThreshold is the minimum confidence score below which a
	// translation is surfaced in the Review UI.
	ConfidenceThreshold float64 `mapstructure:"confidence_threshold" json:"confidence_threshold"`
}

// ResolvedTemperature returns a safe default when Temperature is zero/unset.
// Translation benefits from low-but-not-zero sampling so the output is
// deterministic without collapsing into degenerate repetitions.
func (p LanguagePair) ResolvedTemperature() float64 {
	if p.Temperature <= 0 {
		return 0.2
	}
	if p.Temperature > 2 {
		return 2
	}
	return p.Temperature
}

// Load reads config.yaml (or defaults) and expands paths. It does *not* apply
// the DB overlay — that happens later via ApplyOverlay after we have a DB.
func Load() (*Config, error) {
	viper.SetConfigName("config")
	viper.SetConfigType("yaml")
	// Search cwd, then each ancestor, so `config.yaml` is found when the server is
	// started from backend/ (`make dev`), from a subpackage during `go test`, or
	// from the repo root (`make run`). Without ancestors, cwd=backend/ misses
	// the repo file and Viper falls through to ~/.glyph/config.yaml,
	// which often lists only an older single-model setup (e.g. Viking).
	wd, err := os.Getwd()
	if err != nil {
		wd = "."
	}
	for d, i := wd, 0; i < 8; i++ {
		viper.AddConfigPath(d)
		parent := filepath.Dir(d)
		if parent == d {
			break
		}
		d = parent
	}
	viper.AddConfigPath(os.ExpandEnv("$HOME/.glyph"))

	setDefaults()

	if err := viper.ReadInConfig(); err != nil {
		if _, ok := err.(viper.ConfigFileNotFoundError); !ok {
			return nil, err
		}
		// Config file missing is fine — defaults will be used.
	}

	var cfg Config
	if err := viper.Unmarshal(&cfg); err != nil {
		return nil, fmt.Errorf("failed to unmarshal config: %w", err)
	}

	cfg.App.TranslationsDir = os.ExpandEnv(cfg.App.TranslationsDir)
	cfg.App.ModelsDir = os.ExpandEnv(cfg.App.ModelsDir)
	cfg.App.DBPath = os.ExpandEnv(cfg.App.DBPath)
	cfg.App.LLMBinary = os.ExpandEnv(cfg.App.LLMBinary)
	for i := range cfg.LLM.Models {
		cfg.LLM.Models[i].ModelPath = os.ExpandEnv(cfg.LLM.Models[i].ModelPath)
		cfg.LLM.Models[i].HFRepo = os.ExpandEnv(cfg.LLM.Models[i].HFRepo)
		cfg.LLM.Models[i].HFFile = os.ExpandEnv(cfg.LLM.Models[i].HFFile)
	}

	if err := os.MkdirAll(cfg.App.TranslationsDir, 0o755); err != nil {
		return nil, fmt.Errorf("failed to create translations directory: %w", err)
	}
	if err := os.MkdirAll(cfg.App.ModelsDir, 0o755); err != nil {
		return nil, fmt.Errorf("failed to create models directory: %w", err)
	}
	// Make sure the DB's parent directory exists (the DB file itself will be
	// created by sqlite on open).
	if dir := filepath.Dir(cfg.App.DBPath); dir != "" {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return nil, fmt.Errorf("failed to create db directory: %w", err)
		}
	}

	return &cfg, nil
}

func setDefaults() {
	homeDir, _ := os.UserHomeDir()

	viper.SetDefault("app.port", 3000)
	viper.SetDefault("app.translations_dir", "./translations")
	viper.SetDefault("app.models_dir", filepath.Join(homeDir, ".glyph", "models"))
	viper.SetDefault("app.db_path", "./glyph.db")
	viper.SetDefault("app.llm_binary", "llama-server")

	viper.SetDefault("user_preferences.source_language", "en")
	viper.SetDefault("user_preferences.default_target_language", "sv")

	viper.SetDefault("llm.models", []map[string]interface{}{
		{
			"name":     "default",
			"endpoint": "http://localhost:8000",
			"model":    "mistral",
		},
	})

	viper.SetDefault("language_pairs.en_sv", map[string]interface{}{
		"model":                "default",
		"temperature":          0.2,
		"confidence_threshold": 0.7,
	})
}

// Save writes the current config back to ~/.glyph/config.yaml.
// This is kept for users who prefer to version-control YAML; the canonical
// source of truth for mutable sections is the DB, which should be written to
// alongside this by the caller.
func Save(cfg *Config) error {
	homeDir, err := os.UserHomeDir()
	if err != nil {
		return err
	}

	configDir := filepath.Join(homeDir, ".glyph")
	if err := os.MkdirAll(configDir, 0o755); err != nil {
		return err
	}

	viper.SetConfigFile(filepath.Join(configDir, "config.yaml"))
	viper.Set("app", cfg.App)
	viper.Set("user_preferences", cfg.UserPreferences)
	viper.Set("llm", cfg.LLM)
	viper.Set("language_pairs", cfg.LanguagePairs)

	return viper.WriteConfig()
}

// Overlay is the subset of Config that gets persisted to the DB on UI edits.
// Only fields that the UI is actually allowed to change belong here.
type Overlay struct {
	UserPreferences *UserPreferencesConfig   `json:"user_preferences,omitempty"`
	LanguagePairs   map[string]LanguagePair  `json:"language_pairs,omitempty"`
}

// overlayStore is the minimal interface we need from the db package. Defined
// here to avoid a package import cycle (config <-> db).
type overlayStore interface {
	GetConfigSection(key string, out any) (bool, error)
	SetConfigSection(key string, value any) error
}

// overlayKey is the config_kv row we read/write.
const overlayKey = "config_overlay"

// ApplyOverlay reads the DB overlay (if any) and merges it into cfg in place.
// Safe to call even if the overlay doesn't exist yet.
func ApplyOverlay(cfg *Config, store overlayStore) error {
	var ov Overlay
	found, err := store.GetConfigSection(overlayKey, &ov)
	if err != nil {
		return err
	}
	if !found {
		return nil
	}
	if ov.UserPreferences != nil {
		cfg.UserPreferences = *ov.UserPreferences
	}
	if ov.LanguagePairs != nil {
		// Merge — keys present in the overlay replace YAML entries, others are
		// preserved so language pairs added via YAML aren't wiped.
		if cfg.LanguagePairs == nil {
			cfg.LanguagePairs = make(map[string]LanguagePair)
		}
		for k, v := range ov.LanguagePairs {
			cfg.LanguagePairs[k] = v
		}
	}
	return nil
}

// PersistOverlay writes the mutable sections of cfg to the DB overlay so they
// survive a restart. Called from the UpdateConfig handler.
func PersistOverlay(cfg *Config, store overlayStore) error {
	ov := Overlay{
		UserPreferences: &cfg.UserPreferences,
		LanguagePairs:   cfg.LanguagePairs,
	}
	return store.SetConfigSection(overlayKey, ov)
}
