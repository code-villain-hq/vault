// Package routes holds the HTTP handlers for the public JSON API.
//
// All translation endpoints accept an optional `?ns=<namespace>` query
// parameter. When omitted the default namespace ("translation") is used,
// matching the locize/i18next conventions.
package routes

import (
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/gin-gonic/gin"
	"glyph/internal/config"
	"glyph/internal/db"
	"glyph/internal/llm"
	"glyph/internal/llmproc"
	"glyph/internal/translation"
)

// nsFromQuery reads `?ns=` and falls back to the default namespace.
func nsFromQuery(c *gin.Context) string {
	ns := strings.TrimSpace(c.Query("ns"))
	if ns == "" {
		return translation.DefaultNamespace
	}
	return ns
}

// Health check endpoint.
func Health() gin.HandlerFunc {
	return func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{
			"status": "healthy",
		})
	}
}

// GetLanguages returns all available languages with aggregate counts across
// all namespaces.
//
// "Available" means the union of:
//   - languages that currently have at least one translation in the DB, and
//   - languages referenced as a target in any configured language_pair
//     (so a user who just added a new pair from the UI sees the language
//     appear immediately, even before any translations exist).
//
// The source language is flagged with `is_source: true` so the UI can style
// it differently and forbid deletion.
func GetLanguages(mgr *translation.Manager, cfg *config.Config) gin.HandlerFunc {
	return func(c *gin.Context) {
		seen := make(map[string]struct{})
		for _, l := range mgr.GetLanguages() {
			seen[l] = struct{}{}
		}
		// Language pair keys are "{source}_{target}". Split on the *first*
		// underscore so multi-part codes on the right side (e.g. "zh_CN")
		// still work. Keys that don't look like pairs are skipped silently.
		for key := range cfg.LanguagePairs {
			src, tgt, ok := strings.Cut(key, "_")
			if !ok {
				continue
			}
			if src != "" {
				seen[src] = struct{}{}
			}
			if tgt != "" {
				seen[tgt] = struct{}{}
			}
		}
		// The configured source language is always considered "known" so it
		// shows up in the Settings listing even on a brand-new install.
		if cfg.UserPreferences.SourceLanguage != "" {
			seen[cfg.UserPreferences.SourceLanguage] = struct{}{}
		}

		langs := make([]string, 0, len(seen))
		for l := range seen {
			langs = append(langs, l)
		}
		sort.Strings(langs)

		source := cfg.UserPreferences.SourceLanguage
		result := make([]gin.H, 0, len(langs))
		for _, lang := range langs {
			result = append(result, gin.H{
				"code":              lang,
				"translation_count": mgr.TranslationCountForLang(lang),
				"namespaces":        mgr.GetNamespaces(lang),
				"is_source":         lang == source,
			})
		}

		c.JSON(http.StatusOK, result)
	}
}

// GetNamespaces returns the union of namespaces across all languages, or the
// namespaces for a specific language if `?lang=` is given.
func GetNamespaces(mgr *translation.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		lang := c.Query("lang")
		c.JSON(http.StatusOK, gin.H{
			"namespaces": mgr.GetNamespaces(lang),
		})
	}
}

// GetConfig returns the current, fully-resolved configuration.
func GetConfig(cfg *config.Config) gin.HandlerFunc {
	return func(c *gin.Context) {
		c.JSON(http.StatusOK, gin.H{
			"app": gin.H{
				"port":             cfg.App.Port,
				"translations_dir": cfg.App.TranslationsDir,
				"db_path":          cfg.App.DBPath,
			},
			"user_preferences": cfg.UserPreferences,
			"language_pairs":   cfg.LanguagePairs,
			"llm":              cfg.LLM,
		})
	}
}

// UpdateConfig persists the mutable sections (user_preferences, language_pairs)
// to the SQLite overlay so changes survive a restart. YAML is also written as
// a best-effort.
func UpdateConfig(cfg *config.Config, store *db.DB) gin.HandlerFunc {
	return func(c *gin.Context) {
		var updates map[string]interface{}
		if err := c.BindJSON(&updates); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}

		if prefs, ok := updates["user_preferences"].(map[string]interface{}); ok {
			if source, ok := prefs["source_language"].(string); ok {
				cfg.UserPreferences.SourceLanguage = source
			}
			if target, ok := prefs["default_target_language"].(string); ok {
				cfg.UserPreferences.DefaultTargetLanguage = target
			}
		}

		// language_pairs comes in as a flat map keyed by "{source}_{target}".
		// Each entry is expected to carry { model, temperature, confidence_threshold }.
		// We fully replace cfg.LanguagePairs so the UI can also delete pairs.
		if rawPairs, ok := updates["language_pairs"].(map[string]interface{}); ok {
			pairs := make(map[string]config.LanguagePair, len(rawPairs))
			for key, raw := range rawPairs {
				entry, ok := raw.(map[string]interface{})
				if !ok {
					continue
				}
				p := config.LanguagePair{}
				if m, ok := entry["model"].(string); ok {
					p.Model = m
				}
				if t, ok := entry["temperature"].(float64); ok {
					p.Temperature = t
				}
				if ct, ok := entry["confidence_threshold"].(float64); ok {
					p.ConfidenceThreshold = ct
				}
				pairs[key] = p
			}
			cfg.LanguagePairs = pairs
		}

		if err := config.PersistOverlay(cfg, store); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}

		yamlErr := config.Save(cfg)

		resp := gin.H{"message": "Configuration updated successfully"}
		if yamlErr != nil {
			resp["yaml_save_warning"] = yamlErr.Error()
		}
		c.JSON(http.StatusOK, resp)
	}
}

// GetTranslations returns all translations for (lang, namespace).
func GetTranslations(mgr *translation.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		lang := c.Param("lang")
		ns := nsFromQuery(c)
		translations := mgr.GetTranslations(lang, ns)
		c.JSON(http.StatusOK, translations)
	}
}

// GetTranslationStats returns coverage stats for (source, target, namespace).
func GetTranslationStats(mgr *translation.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		lang := c.Param("lang")
		source := c.DefaultQuery("source", "en")
		ns := nsFromQuery(c)

		stats := mgr.GetLanguagePairStats(source, lang, ns)
		c.JSON(http.StatusOK, stats)
	}
}

// GetMissingTranslations returns the list of missing keys for
// (source, target, namespace).
func GetMissingTranslations(mgr *translation.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		targetLang := c.Param("lang")
		sourceLang := c.DefaultQuery("source", "en")
		ns := nsFromQuery(c)

		missing := mgr.GetMissingTranslations(sourceLang, targetLang, ns)
		source := mgr.GetTranslations(sourceLang, ns)

		result := make([]gin.H, 0, len(missing))
		for _, key := range missing {
			result = append(result, gin.H{
				"key":       key,
				"namespace": ns,
				"source":    source[key],
			})
		}

		c.JSON(http.StatusOK, result)
	}
}

// GetMissingSource returns keys that are filled in some other language but
// empty in (lang, namespace). It powers the Review page's back-fill flow:
// when English (the source) is missing for a key that already has Swedish
// or Finnish, we want to surface it alongside normal missing-target rows so
// the user can fix it in the same workflow. The response items carry the
// sampled `source_lang` so the frontend knows which direction to run the
// LLM in.
func GetMissingSource(mgr *translation.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		lang := c.Param("lang")
		ns := nsFromQuery(c)
		items := mgr.GetMissingSourceKeys(lang, ns)
		c.JSON(http.StatusOK, items)
	}
}

// GetLowConfidenceTranslations returns low-confidence translations for
// (lang, namespace).
func GetLowConfidenceTranslations(mgr *translation.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		lang := c.Param("lang")
		ns := nsFromQuery(c)
		threshold := 0.7

		if t := c.Query("threshold"); t != "" {
			if f, err := strconv.ParseFloat(t, 64); err == nil {
				threshold = f
			}
		}

		lowConf := mgr.GetLowConfidenceTranslations(lang, ns, threshold)
		translations := mgr.GetTranslations(lang, ns)

		result := make([]gin.H, 0, len(lowConf))
		for _, key := range lowConf {
			meta, _ := mgr.GetMetadata(lang, ns, key)
			result = append(result, gin.H{
				"key":           key,
				"namespace":     ns,
				"translation":   translations[key],
				"confidence":    meta.Confidence,
				"translated_by": meta.TranslatedBy,
				"timestamp":     meta.Timestamp,
			})
		}

		c.JSON(http.StatusOK, result)
	}
}

// TranslateSingle translates one (namespace, key) pair via the LLM.
//
// When `dry_run` is true, the result is returned but nothing is written to the
// DB — this is what powers the Review page's "preview → accept/reject" flow,
// where the user sees the proposed translation before committing.
func TranslateSingle(mgr *translation.Manager, cfg *config.Config) gin.HandlerFunc {
	return func(c *gin.Context) {
		var req struct {
			Key        string `json:"key"`
			Namespace  string `json:"namespace"`
			TargetLang string `json:"target_lang"`
			SourceLang string `json:"source_lang"`
			// SourceText, when set, overrides the DB lookup. Used by the
			// Translations page's "magic fill" button which back-translates
			// a target value into the source language — the source cell is
			// empty in that case so a lookup would fail.
			SourceText string `json:"source_text"`
			DryRun     bool   `json:"dry_run"`
		}

		if err := c.BindJSON(&req); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}

		sourceLang := req.SourceLang
		if sourceLang == "" {
			sourceLang = cfg.UserPreferences.SourceLanguage
		}
		ns := req.Namespace
		if ns == "" {
			ns = translation.DefaultNamespace
		}

		// Prefer the caller-supplied source text (back-fill flow). Fall back
		// to the canonical lookup when nothing was provided — the normal
		// forward-translation case.
		sourceText := req.SourceText
		if sourceText == "" {
			sourceTranslations := mgr.GetTranslations(sourceLang, ns)
			existing, ok := sourceTranslations[req.Key]
			if !ok {
				c.JSON(http.StatusBadRequest, gin.H{"error": "key not found in source language / namespace"})
				return
			}
			sourceText = existing
		}

		svc := translation.NewTranslationService(mgr, cfg)
		result, err := svc.TranslateSingleKey(sourceLang, req.TargetLang, ns, req.Key, sourceText)
		if err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}

		if !req.DryRun {
			if err := mgr.SetTranslation(req.TargetLang, ns, req.Key, result.Translation); err != nil {
				c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to save translation"})
				return
			}

			existingMeta, _ := mgr.GetMetadata(req.TargetLang, ns, req.Key)
			meta := translation.MergeLLMMetadataStamp(existingMeta, translation.KeyMetadata{
				Confidence:   result.Confidence,
				TranslatedBy: result.ModelUsed,
				Timestamp:    time.Now().UTC().Format(time.RFC3339),
			})
			_ = mgr.SetMetadata(req.TargetLang, ns, req.Key, meta)
		}

		c.JSON(http.StatusOK, result)
	}
}

// TranslateMissing fills every missing key in a namespace for each target.
func TranslateMissing(mgr *translation.Manager, cfg *config.Config) gin.HandlerFunc {
	return func(c *gin.Context) {
		var req struct {
			TargetLangs []string `json:"target_langs"`
			SourceLang  string   `json:"source_lang"`
			Namespace   string   `json:"namespace"`
		}

		if err := c.BindJSON(&req); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}

		sourceLang := req.SourceLang
		if sourceLang == "" {
			sourceLang = cfg.UserPreferences.SourceLanguage
		}
		if len(req.TargetLangs) == 0 {
			c.JSON(http.StatusBadRequest, gin.H{"error": "no target languages specified"})
			return
		}

		svc := translation.NewTranslationService(mgr, cfg)
		results := svc.TranslateMissingKeys(sourceLang, req.TargetLangs, req.Namespace)

		c.JSON(http.StatusOK, gin.H{
			"message": "Translation completed successfully",
			"results": results,
		})
	}
}

// GetMetadata returns every metadata row for (lang, namespace).
func GetMetadata(mgr *translation.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		lang := c.Param("lang")
		ns := nsFromQuery(c)

		metadata := mgr.GetAllMetadataForNamespace(lang, ns)

		c.JSON(http.StatusOK, gin.H{
			"language":  lang,
			"namespace": ns,
			"metadata":  metadata,
		})
	}
}

// UpdateMetadata upserts a single metadata row.
func UpdateMetadata(mgr *translation.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		lang := c.Param("lang")
		key := c.Param("key")
		ns := nsFromQuery(c)

		var meta translation.KeyMetadata
		if err := c.BindJSON(&meta); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}

		if err := mgr.SetMetadata(lang, ns, key, meta); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}

		c.JSON(http.StatusOK, gin.H{"message": "Metadata updated successfully"})
	}
}

// UpdateTranslation updates a single translation value (and optional meta).
func UpdateTranslation(mgr *translation.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		lang := c.Param("lang")
		key := c.Param("key")
		ns := nsFromQuery(c)

		var req struct {
			Translation string                  `json:"translation"`
			Metadata    translation.KeyMetadata `json:"metadata"`
		}

		if err := c.BindJSON(&req); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}

		if err := mgr.SetTranslation(lang, ns, key, req.Translation); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to save translation"})
			return
		}

		if req.Metadata.TranslatedBy != "" || req.Metadata.Timestamp != "" {
			if err := mgr.SetMetadata(lang, ns, key, req.Metadata); err != nil {
				c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to save metadata"})
				return
			}
		}

		c.JSON(http.StatusOK, gin.H{"message": "Translation updated successfully"})
	}
}

// DeleteKey removes a (namespace, key) tuple from every language. Query
// params: `ns` (optional, defaults to DefaultNamespace) and `key` (required).
// Used by the Translations page's "delete key" trash icon.
func DeleteKey(mgr *translation.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		key := c.Query("key")
		ns := nsFromQuery(c)
		if key == "" {
			c.JSON(http.StatusBadRequest, gin.H{"error": "query param 'key' is required"})
			return
		}
		affected, err := mgr.DeleteKey(ns, key)
		if err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}
		c.JSON(http.StatusOK, gin.H{
			"message":            "Key deleted",
			"namespace":          ns,
			"key":                key,
			"affected_languages": affected,
		})
	}
}

// ---------------------------------------------------------------------------
// Namespace management
// ---------------------------------------------------------------------------

// GetNamespaceStats returns per-namespace key + translation counts summed
// across every language. Used by the Translations page's "Manage namespaces"
// modal so users can see how populated each bucket is before they rename or
// delete it.
func GetNamespaceStats(mgr *translation.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		stats := mgr.GetNamespaceStats()
		c.JSON(http.StatusOK, gin.H{"namespaces": stats})
	}
}

// RenameNamespace re-buckets every key under the URL's `:ns` into the JSON
// body's `to`, across every language. Refuses to merge into an existing
// namespace (the underlying DB call enforces this) so users don't silently
// overwrite keys.
func RenameNamespace(mgr *translation.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		oldNS := c.Param("ns")
		if oldNS == "" {
			c.JSON(http.StatusBadRequest, gin.H{"error": "namespace path param is required"})
			return
		}
		var req struct {
			To string `json:"to"`
		}
		if err := c.ShouldBindJSON(&req); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}
		newNS := req.To
		if newNS == "" {
			c.JSON(http.StatusBadRequest, gin.H{"error": "body field 'to' is required"})
			return
		}
		affected, err := mgr.RenameNamespace(oldNS, newNS)
		if err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}
		c.JSON(http.StatusOK, gin.H{
			"message":            "Namespace renamed",
			"from":               oldNS,
			"to":                 newNS,
			"affected_languages": affected,
		})
	}
}

// DeleteNamespace wipes every key in the URL's `:ns` across every language
// and removes the on-disk JSON files. Returns the list of languages that
// actually had rows so the frontend can refresh accordingly.
func DeleteNamespace(mgr *translation.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		ns := c.Param("ns")
		if ns == "" {
			c.JSON(http.StatusBadRequest, gin.H{"error": "namespace path param is required"})
			return
		}
		affected, err := mgr.DeleteNamespace(ns)
		if err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}
		c.JSON(http.StatusOK, gin.H{
			"message":            "Namespace deleted",
			"namespace":          ns,
			"affected_languages": affected,
		})
	}
}

// ---------------------------------------------------------------------------
// Glossary
// ---------------------------------------------------------------------------

// ListGlossary returns the full set of glossary entries, optionally filtered
// by source_lang and target_lang query parameters.
func ListGlossary(mgr *translation.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		entries, err := mgr.ListGlossary(c.Query("source_lang"), c.Query("target_lang"))
		if err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}
		if entries == nil {
			entries = []translation.GlossaryEntry{}
		}
		c.JSON(http.StatusOK, gin.H{"entries": entries, "count": len(entries)})
	}
}

// AddGlossaryEntry creates a new glossary rule.
func AddGlossaryEntry(mgr *translation.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		var entry translation.GlossaryEntry
		if err := c.BindJSON(&entry); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}
		created, err := mgr.AddGlossaryEntry(entry)
		if err != nil {
			// Duplicate rows surface as a SQLite UNIQUE constraint error,
			// which we map to 409 so the UI can show a friendly "this term
			// already exists for this pair" message.
			status := http.StatusInternalServerError
			if strings.Contains(err.Error(), "UNIQUE constraint failed") {
				status = http.StatusConflict
				err = fmt.Errorf("%q is already in the glossary for %s→%s", entry.SourceTerm, entry.SourceLang, entry.TargetLang)
			}
			c.JSON(status, gin.H{"error": err.Error()})
			return
		}
		c.JSON(http.StatusCreated, created)
	}
}

// UpdateGlossaryEntry overwrites the source_term / target_term / note of an
// existing entry. Lang pair is immutable.
func UpdateGlossaryEntry(mgr *translation.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		id, err := strconv.ParseInt(c.Param("id"), 10, 64)
		if err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": "id must be a positive integer"})
			return
		}
		var entry translation.GlossaryEntry
		if err := c.BindJSON(&entry); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}
		if err := mgr.UpdateGlossaryEntry(id, entry); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}
		c.JSON(http.StatusOK, gin.H{"message": "Glossary entry updated"})
	}
}

// DeleteGlossaryEntry removes a single entry by id.
func DeleteGlossaryEntry(mgr *translation.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		id, err := strconv.ParseInt(c.Param("id"), 10, 64)
		if err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": "id must be a positive integer"})
			return
		}
		if err := mgr.DeleteGlossaryEntry(id); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}
		c.JSON(http.StatusOK, gin.H{"message": "Glossary entry removed"})
	}
}

// SuggestGlossaryVariants asks the LLM to list common grammatical forms of
// the glossary entry's target term (definite, plural, genitive, ...). Used
// by the "Find variants" button on the Glossary page so reviewers don't
// hand-type every inflection themselves.
//
// Request body: {source_lang, target_lang, source_term, target_term}.
// Response: {variants: [...]}. An empty list is a legitimate answer (the
// language has no inflection, or the model had nothing useful to add).
func SuggestGlossaryVariants(mgr *translation.Manager, cfg *config.Config) gin.HandlerFunc {
	return func(c *gin.Context) {
		var req struct {
			SourceLang string `json:"source_lang"`
			TargetLang string `json:"target_lang"`
			SourceTerm string `json:"source_term"`
			TargetTerm string `json:"target_term"`
		}
		if err := c.BindJSON(&req); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}
		if req.TargetLang == "" || req.TargetTerm == "" {
			c.JSON(http.StatusBadRequest, gin.H{"error": "target_lang and target_term are required"})
			return
		}
		// source_lang / source_term are optional — the model uses them only
		// as extra context when it's unclear which sense of the target the
		// reviewer intends. Default them to the target side so the prompt
		// still reads coherently if the caller didn't supply them.
		if req.SourceLang == "" {
			req.SourceLang = req.TargetLang
		}
		if req.SourceTerm == "" {
			req.SourceTerm = req.TargetTerm
		}

		svc := translation.NewTranslationService(mgr, cfg)
		variants, err := svc.SuggestGlossaryVariants(req.SourceLang, req.TargetLang, req.SourceTerm, req.TargetTerm)
		if err != nil {
			c.JSON(http.StatusBadGateway, gin.H{"error": err.Error()})
			return
		}
		if variants == nil {
			variants = []string{}
		}
		c.JSON(http.StatusOK, gin.H{
			"source_lang": req.SourceLang,
			"target_lang": req.TargetLang,
			"target_term": req.TargetTerm,
			"variants":    variants,
		})
	}
}

// DeleteLanguage removes a language from the app entirely. It's the one-click
// undo for "I added this language by mistake" and for "we no longer ship X":
//
//  1. All translations + metadata rows are dropped from the DB and the
//     on-disk `translations/{lang}/` folder is deleted.
//  2. Every glossary entry that mentions the language (as source OR target)
//     is removed — rules for a language we no longer speak are dead weight.
//  3. Every language_pair that references the language (in either slot of
//     the "{source}_{target}" key) is removed from the config overlay.
//  4. If user_preferences.default_target_language was set to the deleted
//     language, it's cleared so the UI falls back to picking another target.
//
// The source language (user_preferences.source_language) can't be deleted —
// removing it would leave the app in an undefined state. Callers get a 400.
func DeleteLanguage(mgr *translation.Manager, cfg *config.Config, store *db.DB) gin.HandlerFunc {
	return func(c *gin.Context) {
		lang := strings.TrimSpace(c.Param("lang"))
		if lang == "" {
			c.JSON(http.StatusBadRequest, gin.H{"error": "lang is required"})
			return
		}
		if lang == cfg.UserPreferences.SourceLanguage {
			c.JSON(http.StatusBadRequest, gin.H{
				"error": fmt.Sprintf("%q is the source language and can't be removed. Change it in Settings first.", lang),
			})
			return
		}

		// 1. Translations + metadata + filesystem.
		if err := mgr.DeleteLanguage(lang); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}

		// 2. Glossary entries involving this language.
		glossaryRemoved, gErr := mgr.DeleteGlossaryEntriesForLang(lang)
		if gErr != nil {
			// Non-fatal: the translation delete already succeeded. Surface
			// it in the response so the UI can warn the user.
			glossaryRemoved = 0
		}

		// 3. Language pairs referencing this language, in either slot.
		pairsRemoved := 0
		for key := range cfg.LanguagePairs {
			src, tgt, ok := strings.Cut(key, "_")
			if !ok {
				continue
			}
			if src == lang || tgt == lang {
				delete(cfg.LanguagePairs, key)
				pairsRemoved++
			}
		}

		// 4. Default target fallback.
		if cfg.UserPreferences.DefaultTargetLanguage == lang {
			cfg.UserPreferences.DefaultTargetLanguage = ""
		}

		// Persist the mutated config overlay so changes survive a restart.
		// We try the best-effort YAML write too, matching UpdateConfig.
		if err := config.PersistOverlay(cfg, store); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{
				"error": fmt.Sprintf("language removed from DB but overlay persist failed: %v", err),
			})
			return
		}
		_ = config.Save(cfg)

		resp := gin.H{
			"message":           fmt.Sprintf("Language %q removed from the app.", lang),
			"lang":              lang,
			"glossary_removed":  glossaryRemoved,
			"pairs_removed":     pairsRemoved,
		}
		if gErr != nil {
			resp["glossary_error"] = gErr.Error()
		}
		c.JSON(http.StatusOK, resp)
	}
}

// addLanguageRequest is the JSON body for POST /api/languages.
type addLanguageRequest struct {
	// Code is the ISO-ish language code to add (e.g. "sv", "no", "pt-BR").
	// Required. Trimmed, lower-cased, and validated against a conservative
	// character set so a typo can't sneak a path fragment into the DB.
	Code string `json:"code"`
	// Model is an optional LLM model name to pin to the auto-created pair.
	// If empty, the first configured model is used at translation time.
	Model string `json:"model"`
}

// AddLanguage registers a new target language with the app.
//
// The actual "adding" is a thin operation: we create a language_pair entry
// `{source}_{code}` (if one doesn't already exist) and persist it to the
// overlay. That alone is enough for the language to show up in the UI
// everywhere (/api/languages now unions configured pair targets), so the
// user can immediately open Translations or Review and start filling it in.
//
// No translations are seeded — leaving the language at "0 translated" is
// the correct initial state and avoids creating placeholder rows the
// review flow would have to special-case.
func AddLanguage(cfg *config.Config, store *db.DB) gin.HandlerFunc {
	return func(c *gin.Context) {
		var req addLanguageRequest
		if err := c.BindJSON(&req); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}
		code := strings.ToLower(strings.TrimSpace(req.Code))
		if code == "" {
			c.JSON(http.StatusBadRequest, gin.H{"error": "code is required"})
			return
		}
		if !isValidLangCode(code) {
			c.JSON(http.StatusBadRequest, gin.H{
				"error": "invalid language code — use letters, digits, '-' or '_' (e.g. \"sv\", \"pt-br\")",
			})
			return
		}
		source := cfg.UserPreferences.SourceLanguage
		if source == "" {
			c.JSON(http.StatusBadRequest, gin.H{
				"error": "user_preferences.source_language is not set — configure it before adding languages",
			})
			return
		}
		if code == source {
			c.JSON(http.StatusBadRequest, gin.H{
				"error": fmt.Sprintf("%q is already the source language", code),
			})
			return
		}

		if cfg.LanguagePairs == nil {
			cfg.LanguagePairs = make(map[string]config.LanguagePair)
		}
		pairKey := source + "_" + code
		created := false
		if _, exists := cfg.LanguagePairs[pairKey]; !exists {
			// Sensible defaults — user can tune later in Settings.
			cfg.LanguagePairs[pairKey] = config.LanguagePair{
				Model:               req.Model,
				Temperature:         0,   // → 0.2 via ResolvedTemperature
				ConfidenceThreshold: 0.7, // matches default shown in Settings
			}
			created = true
		}

		if err := config.PersistOverlay(cfg, store); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{
				"error": fmt.Sprintf("failed to persist overlay: %v", err),
			})
			return
		}
		_ = config.Save(cfg)

		c.JSON(http.StatusOK, gin.H{
			"message":      fmt.Sprintf("Language %q is ready. Start translating from the Translations page.", code),
			"code":         code,
			"pair":         pairKey,
			"pair_created": created,
		})
	}
}

// isValidLangCode enforces a conservative charset for user-supplied language
// codes. We allow letters, digits, `-` and `_` (the latter only because some
// legacy code uses `zh_CN`-style keys). Length is bounded so the value fits
// comfortably into logs, URLs and filesystem paths.
func isValidLangCode(s string) bool {
	if len(s) == 0 || len(s) > 15 {
		return false
	}
	for i := 0; i < len(s); i++ {
		b := s[i]
		switch {
		case b >= 'a' && b <= 'z':
		case b >= '0' && b <= '9':
		case b == '-' || b == '_':
		default:
			return false
		}
	}
	return true
}

// SearchTranslations does a naive case-insensitive substring search over
// translations. Always searches every namespace.
func SearchTranslations(mgr *translation.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		query := strings.ToLower(c.Query("q"))
		lang := c.Query("lang")

		type SearchResult struct {
			Lang        string  `json:"lang"`
			Namespace   string  `json:"namespace"`
			Key         string  `json:"key"`
			Source      string  `json:"source"`
			Translation string  `json:"translation"`
			Confidence  float64 `json:"confidence"`
		}

		langs := []string{lang}
		if lang == "" {
			langs = mgr.GetLanguages()
		}

		results := make([]SearchResult, 0)
		for _, l := range langs {
			for _, ns := range mgr.GetNamespaces(l) {
				trans := mgr.GetTranslations(l, ns)
				source := mgr.GetTranslations("en", ns)
				for key, value := range trans {
					if strings.Contains(strings.ToLower(key), query) || strings.Contains(strings.ToLower(value), query) {
						meta, _ := mgr.GetMetadata(l, ns, key)
						results = append(results, SearchResult{
							Lang:        l,
							Namespace:   ns,
							Key:         key,
							Source:      source[key],
							Translation: value,
							Confidence:  meta.Confidence,
						})
					}
				}
			}
		}

		c.JSON(http.StatusOK, results)
	}
}

// PreviewImport parses an uploaded JSON file and returns the namespace
// breakdown and sample rows that would be written — without touching the
// database. Used by the Import page's review step so reviewers confirm
// locize-style `ns:key` routing before merging.
func PreviewImport() gin.HandlerFunc {
	return func(c *gin.Context) {
		file, err := c.FormFile("file")
		if err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": "no file provided"})
			return
		}

		defaultNS := c.PostForm("namespace")
		if defaultNS == "" {
			defaultNS = translation.DefaultNamespace
		}

		src, err := file.Open()
		if err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to open file"})
			return
		}
		defer src.Close()

		data, err := io.ReadAll(src)
		if err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to read file"})
			return
		}

		var translations map[string]string
		if err := json.Unmarshal(data, &translations); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": "invalid JSON format"})
			return
		}

		keys := make([]string, 0, len(translations))
		for k := range translations {
			keys = append(keys, k)
		}
		sort.Strings(keys)

		byNS := make(map[string]int)
		const maxSamples = 50
		samples := make([]gin.H, 0, maxSamples)
		for _, key := range keys {
			value := translations[key]
			ns, realKey := splitLocizeKey(key, defaultNS)
			byNS[ns]++
			if len(samples) < maxSamples {
				samples = append(samples, gin.H{
					"namespace":     ns,
					"key":           realKey,
					"raw_key":       key,
					"value_preview": truncateImportPreview(value, 120),
				})
			}
		}

		type nsBreakdown struct {
			Namespace string `json:"namespace"`
			Count     int    `json:"count"`
		}
		breakdown := make([]nsBreakdown, 0, len(byNS))
		for ns, n := range byNS {
			breakdown = append(breakdown, nsBreakdown{Namespace: ns, Count: n})
		}
		sort.Slice(breakdown, func(i, j int) bool { return breakdown[i].Namespace < breakdown[j].Namespace })

		c.JSON(http.StatusOK, gin.H{
			"default_namespace": defaultNS,
			"count":             len(translations),
			"namespaces":        breakdown,
			"sample_rows":       samples,
		})
	}
}

func truncateImportPreview(s string, maxRunes int) string {
	s = strings.TrimSpace(s)
	if s == "" {
		return ""
	}
	r := []rune(s)
	if len(r) <= maxRunes {
		return s
	}
	return string(r[:maxRunes]) + "…"
}

// ImportTranslations imports a JSON translation file into `(lang, namespace)`.
//
// The importer understands two key layouts in the same pass:
//
//  1. Flat, single-namespace: `{ "login.title": "Sign in", ... }`. Every key
//     lands in the form's `namespace` (default: "translation").
//  2. Locize-style, multi-namespace: `{ "billing:invoiceDueSoon": "...", "common:ok": "..." }`.
//     Keys with a `namespace:key` prefix are re-bucketed into their embedded
//     namespace — the form's `namespace` is used only as the fallback for
//     keys that DON'T have a prefix. We split on the FIRST colon so keys
//     containing colons further in (e.g. `url:help:en`) still work once the
//     leading `url:` has been peeled off.
//
// The `namespace:` prefix is only honoured when the part before the colon
// looks like a valid namespace identifier (letters, digits, dot, dash,
// underscore). That guards against false positives from keys whose value
// simply contains a colon (e.g. `time_format: "HH:mm"` would never parse
// as locize because the colon is inside the VALUE, but we also don't want
// `https://example.com` to be treated as namespace `https`).
func ImportTranslations(mgr *translation.Manager, cfg *config.Config) gin.HandlerFunc {
	return func(c *gin.Context) {
		file, err := c.FormFile("file")
		if err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": "no file provided"})
			return
		}

		lang := c.PostForm("lang")
		defaultNS := c.PostForm("namespace")
		if defaultNS == "" {
			defaultNS = translation.DefaultNamespace
		}
		mode := c.DefaultPostForm("mode", "merge")

		if lang == "" {
			c.JSON(http.StatusBadRequest, gin.H{"error": "language code required"})
			return
		}

		src, err := file.Open()
		if err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to open file"})
			return
		}
		defer src.Close()

		data, err := io.ReadAll(src)
		if err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": "failed to read file"})
			return
		}

		var translations map[string]string
		if err := json.Unmarshal(data, &translations); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": "invalid JSON format"})
			return
		}

		// "replace" mode is a no-op at the key level; existing keys are
		// overwritten by the loop below. A full-replace would need to delete
		// all other keys in the namespace first, which we don't do yet.
		_ = mode

		// byNS groups writes so we can report a per-namespace breakdown in
		// the response — much more useful than a single aggregate count when
		// the same file spans multiple namespaces.
		byNS := make(map[string]int)
		for key, value := range translations {
			ns, realKey := splitLocizeKey(key, defaultNS)
			if err := mgr.SetTranslation(lang, ns, realKey, value); err != nil {
				c.JSON(http.StatusInternalServerError, gin.H{"error": fmt.Sprintf("failed to save key %s: %v", key, err)})
				return
			}
			byNS[ns]++
		}

		// Flatten the per-namespace breakdown into a stable, sorted slice so
		// clients see deterministic output and can render it directly.
		type nsBreakdown struct {
			Namespace string `json:"namespace"`
			Count     int    `json:"count"`
		}
		breakdown := make([]nsBreakdown, 0, len(byNS))
		for ns, n := range byNS {
			breakdown = append(breakdown, nsBreakdown{Namespace: ns, Count: n})
		}
		sort.Slice(breakdown, func(i, j int) bool { return breakdown[i].Namespace < breakdown[j].Namespace })

		// The top-level `namespace` field echoes the DEFAULT namespace used
		// (the form value). The authoritative per-namespace split is in
		// `namespaces`. This keeps backward compatibility with clients that
		// only read `namespace` while newer clients can render the breakdown.
		c.JSON(http.StatusOK, gin.H{
			"message":    fmt.Sprintf("Imported %d keys into %s across %d namespace(s)", len(translations), lang, len(byNS)),
			"lang":       lang,
			"namespace":  defaultNS,
			"namespaces": breakdown,
			"count":      len(translations),
		})
	}
}

// splitLocizeKey peels a leading `namespace:` off `raw` if one is present.
// Falls back to the caller's default namespace when no valid prefix exists.
//
// A valid prefix is `[A-Za-z0-9._-]+` followed by a colon. Anything else
// (empty prefix, spaces, colons inside the value) is treated as a plain key.
func splitLocizeKey(raw, defaultNS string) (ns, key string) {
	idx := strings.IndexByte(raw, ':')
	if idx <= 0 {
		return defaultNS, raw
	}
	prefix := raw[:idx]
	if !isValidNamespaceIdent(prefix) {
		return defaultNS, raw
	}
	rest := raw[idx+1:]
	if rest == "" {
		// `billing:` with no key after — not locize format, treat the whole
		// string as a key in the default namespace.
		return defaultNS, raw
	}
	// Reject URL-shaped inputs: a second slash immediately after the colon
	// means this is almost certainly `https://…` / `ws://…` / `file://…`
	// rather than a `namespace:key` pair. Real locize keys never start
	// with a slash.
	if rest[0] == '/' {
		return defaultNS, raw
	}
	return prefix, rest
}

// isValidNamespaceIdent reports whether s is a plausible namespace name:
// non-empty and consisting only of ASCII letters, digits, dot, dash, or
// underscore. Matches the guard used in the namespace management UI.
func isValidNamespaceIdent(s string) bool {
	if s == "" {
		return false
	}
	for i := 0; i < len(s); i++ {
		b := s[i]
		switch {
		case b >= 'A' && b <= 'Z':
		case b >= 'a' && b <= 'z':
		case b >= '0' && b <= '9':
		case b == '.' || b == '-' || b == '_':
		default:
			return false
		}
	}
	return true
}

// LLMTest tests if an LLM endpoint is reachable.
func LLMTest(cfg *config.Config) gin.HandlerFunc {
	return func(c *gin.Context) {
		endpoint := c.Query("endpoint")
		if endpoint == "" {
			c.JSON(http.StatusBadRequest, gin.H{"error": "endpoint required"})
			return
		}

		reachable, status, err := pingLLM(endpoint)
		resp := gin.H{"reachable": reachable, "status": status}
		if err != nil {
			resp["error"] = err.Error()
		}
		c.JSON(http.StatusOK, resp)
	}
}

// pingLLM returns (reachable, httpStatus, err). A reachable server is one
// whose /health returns 200.
func pingLLM(endpoint string) (bool, int, error) {
	client := &http.Client{Timeout: 2 * time.Second}
	base := llm.NormalizeLLMEndpoint(endpoint)
	resp, err := client.Get(base + "/health")
	if err != nil {
		return false, 0, err
	}
	defer resp.Body.Close()
	return resp.StatusCode == http.StatusOK, resp.StatusCode, nil
}

// findModel looks up an LLM config by name. Returns nil if not found.
func findModel(cfg *config.Config, name string) *config.LLMModel {
	for i := range cfg.LLM.Models {
		if cfg.LLM.Models[i].Name == name {
			return &cfg.LLM.Models[i]
		}
	}
	return nil
}

// LLMStatus reports, for a named LLM model, whether its endpoint is reachable
// and whether we're actively managing the process.
func LLMStatus(cfg *config.Config, procs *llmproc.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		name := c.Query("name")
		if name == "" {
			c.JSON(http.StatusBadRequest, gin.H{"error": "name required"})
			return
		}
		m := findModel(cfg, name)
		if m == nil {
			c.JSON(http.StatusNotFound, gin.H{"error": "model not configured"})
			return
		}

		reachable, _, pingErr := pingLLM(m.Endpoint)
		status := procs.Status(name)

		resp := gin.H{
			"name":      name,
			"endpoint":  m.Endpoint,
			"reachable": reachable,
			"managed":   status != nil,
			"can_start": m.CanStart(),
		}
		if pingErr != nil && !reachable {
			resp["ping_error"] = pingErr.Error()
		}
		if status != nil {
			resp["pid"] = status.PID
			resp["started_at"] = status.StartedAt
			resp["exited"] = status.Exited
			if status.ExitError != "" {
				resp["exit_error"] = status.ExitError
			}
			if len(status.Recent) > 0 {
				resp["recent_output"] = status.Recent
			}
		}
		c.JSON(http.StatusOK, resp)
	}
}

// LLMStart spawns the configured LLM binary for a named model. The port is
// taken from the model's endpoint URL so the running server lines up with
// what the rest of the app is already pointing at.
func LLMStart(cfg *config.Config, procs *llmproc.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		var req struct {
			Name string `json:"name"`
		}
		if err := c.BindJSON(&req); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}
		if req.Name == "" {
			c.JSON(http.StatusBadRequest, gin.H{"error": "name required"})
			return
		}

		m := findModel(cfg, req.Name)
		if m == nil {
			c.JSON(http.StatusNotFound, gin.H{"error": "model not configured"})
			return
		}
		hfRepo := strings.TrimSpace(m.HFRepo)
		hfFile := strings.TrimSpace(m.HFFile)
		if hfRepo != "" || hfFile != "" {
			if hfRepo == "" || hfFile == "" {
				c.JSON(http.StatusBadRequest, gin.H{
					"error": "hf_repo and hf_file must both be set (or omit both and use model_path) for Hugging Face download mode",
				})
				return
			}
		} else if strings.TrimSpace(m.ModelPath) == "" {
			c.JSON(http.StatusBadRequest, gin.H{
				"error": "set model_path to a local GGUF, or hf_repo + hf_file for llama-server --hf download mode",
			})
			return
		}

		port, err := portFromEndpoint(m.Endpoint)
		if err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": fmt.Sprintf("invalid endpoint %q: %v", m.Endpoint, err)})
			return
		}

		binary := cfg.App.LLMBinary
		if binary == "" {
			binary = "llama-server"
		}

		var argv []string
		if m.HFDownloadMode() {
			argv = []string{"--hf-repo", hfRepo, "--hf-file", hfFile, "--port", strconv.Itoa(port)}
		} else {
			argv = []string{"-m", m.ModelPath, "--port", strconv.Itoa(port)}
		}
		argv = append(argv, m.ExtraArgs...)

		status, err := procs.Start(req.Name, binary, argv)
		if err != nil {
			code := http.StatusInternalServerError
			if err == llmproc.ErrAlreadyRunning {
				code = http.StatusConflict
			}
			c.JSON(code, gin.H{"error": err.Error()})
			return
		}

		c.JSON(http.StatusOK, gin.H{
			"message": "LLM server starting",
			"name":    req.Name,
			"pid":     status.PID,
			"argv":    append([]string{binary}, argv...),
		})
	}
}

// LLMStop stops a managed LLM process. External processes (started outside
// this app) cannot be stopped this way.
func LLMStop(procs *llmproc.Manager) gin.HandlerFunc {
	return func(c *gin.Context) {
		var req struct {
			Name string `json:"name"`
		}
		if err := c.BindJSON(&req); err != nil {
			c.JSON(http.StatusBadRequest, gin.H{"error": err.Error()})
			return
		}
		if req.Name == "" {
			c.JSON(http.StatusBadRequest, gin.H{"error": "name required"})
			return
		}

		if err := procs.Stop(req.Name, 5*time.Second); err != nil {
			if err == llmproc.ErrNotRunning {
				c.JSON(http.StatusOK, gin.H{"message": "not running"})
				return
			}
			c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
			return
		}
		c.JSON(http.StatusOK, gin.H{"message": "LLM server stopped"})
	}
}

// portFromEndpoint extracts the TCP port from a URL like http://host:1234.
// llama-server binds to a port, so having one in the endpoint is required.
func portFromEndpoint(endpoint string) (int, error) {
	u, err := url.Parse(endpoint)
	if err != nil {
		return 0, err
	}
	p := u.Port()
	if p == "" {
		return 0, fmt.Errorf("endpoint must include an explicit port")
	}
	port, err := strconv.Atoi(p)
	if err != nil {
		return 0, err
	}
	return port, nil
}
