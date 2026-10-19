// Package translation is the translation/metadata service layer.
//
// Storage model (locize/i18next-style):
//
//   - SQLite is the source of truth. Every row is keyed by
//     (lang, namespace, key) — namespaces group related strings together
//     (e.g. "common", "auth", "dashboard"), mirroring how locize and
//     react-i18next organise translations.
//
//   - On writes we also export JSON files so downstream i18n tooling keeps
//     working. Layout:
//
//         translations/{lang}/{namespace}.json   — preferred, namespaced
//         translations/{lang}.json               — legacy flat layout
//
//     We read both on bootstrap: a nested `{lang}/{ns}.json` seeds namespace
//     `ns`, and any top-level `{lang}.json` seeds the default namespace
//     ("translation"). After that, the DB wins.
package translation

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"

	"glyph/internal/db"
)

// DefaultNamespace is re-exported for callers that want to avoid importing
// the db package just for this constant.
const DefaultNamespace = db.DefaultNamespace

// Metadata is the on-disk structure of translations.meta.json.
// We keep emitting this file for backwards compatibility: it nests
// lang -> namespace -> key -> meta so tools can still read it even once
// namespaces are in play.
type Metadata struct {
	Translations map[string]map[string]map[string]KeyMetadata `json:"translations"`
}

// KeyMetadata is the per-(lang, namespace, key) metadata row.
type KeyMetadata struct {
	Confidence   float64 `json:"confidence"`
	TranslatedBy string  `json:"translated_by"`
	Timestamp    string  `json:"timestamp"`
	// Context is reviewer-written UI/source context (where the string appears,
	// tone, etc.). It is injected into LLM prompts for this key; usually edited
	// on the source-language row in the UI and preserved when MT re-stamps
	// confidence/translator fields.
	Context string `json:"context,omitempty"`
}

// MergeLLMMetadataStamp copies Context from existing onto stamp so LLM-only
// updates (confidence / translated_by / timestamp) never wipe reviewer text.
func MergeLLMMetadataStamp(existing, stamp KeyMetadata) KeyMetadata {
	stamp.Context = existing.Context
	return stamp
}

// Manager owns the in-memory cache and the DB handle.
//
// Cache layout: lang -> namespace -> key -> value (and analogous for meta).
// Reads are served from the cache; writes go to the DB first, then update the
// cache, then re-export the affected JSON file.
type Manager struct {
	translationsDir string
	store           *db.DB

	mu           sync.RWMutex
	translations map[string]map[string]map[string]string      // lang -> ns -> key -> value
	metadata     map[string]map[string]map[string]KeyMetadata // lang -> ns -> key -> meta
}

// NewManager opens/creates the translation state. If the DB is empty it seeds
// itself from whatever JSON files are already in translationsDir.
func NewManager(translationsDir string, store *db.DB) (*Manager, error) {
	m := &Manager{
		translationsDir: translationsDir,
		store:           store,
		translations:    make(map[string]map[string]map[string]string),
		metadata:        make(map[string]map[string]map[string]KeyMetadata),
	}

	count, err := store.CountTranslations()
	if err != nil {
		return nil, fmt.Errorf("count translations: %w", err)
	}
	if count == 0 {
		if err := m.bootstrapFromJSON(); err != nil {
			return nil, fmt.Errorf("bootstrap from json: %w", err)
		}
	}

	if err := m.reloadCache(); err != nil {
		return nil, err
	}
	return m, nil
}

// bootstrapFromJSON seeds the DB from any JSON files already on disk.
//
// Two layouts are supported on disk, and both are honoured on bootstrap:
//
//	translations/{lang}.json          -> namespace "translation"
//	translations/{lang}/{ns}.json     -> namespace ns
func (m *Manager) bootstrapFromJSON() error {
	entries, err := os.ReadDir(m.translationsDir)
	if err != nil {
		if os.IsNotExist(err) {
			return nil
		}
		return fmt.Errorf("read translations dir: %w", err)
	}

	for _, entry := range entries {
		name := entry.Name()

		if entry.IsDir() {
			// New-style layout: translations/{lang}/{ns}.json
			lang := name
			subDir := filepath.Join(m.translationsDir, lang)
			subEntries, err := os.ReadDir(subDir)
			if err != nil {
				return fmt.Errorf("read %s: %w", subDir, err)
			}
			for _, f := range subEntries {
				if f.IsDir() || !strings.HasSuffix(f.Name(), ".json") {
					continue
				}
				ns := strings.TrimSuffix(f.Name(), ".json")
				path := filepath.Join(subDir, f.Name())
				if err := m.seedFile(lang, ns, path); err != nil {
					return err
				}
			}
			continue
		}

		if !strings.HasSuffix(name, ".json") {
			continue
		}
		if name == "translations.meta.json" {
			// Handled below.
			continue
		}

		// Legacy flat layout: translations/{lang}.json
		lang := strings.TrimSuffix(name, ".json")
		path := filepath.Join(m.translationsDir, name)
		if err := m.seedFile(lang, DefaultNamespace, path); err != nil {
			return err
		}
	}

	// Seed metadata from translations.meta.json, if present. The file may
	// contain either the v1 layout (lang -> key -> meta) or the new v2 layout
	// (lang -> namespace -> key -> meta). We try v2 first and fall back.
	metaPath := filepath.Join(m.translationsDir, "translations.meta.json")
	if data, err := os.ReadFile(metaPath); err == nil {
		if err := m.seedMetadata(data); err != nil {
			return fmt.Errorf("seed meta: %w", err)
		}
	}

	return nil
}

// seedFile reads a single key->value JSON file into the DB under
// (lang, namespace).
func (m *Manager) seedFile(lang, namespace, path string) error {
	data, err := os.ReadFile(path)
	if err != nil {
		return fmt.Errorf("read %s: %w", path, err)
	}
	var kv map[string]string
	if err := json.Unmarshal(data, &kv); err != nil {
		return fmt.Errorf("parse %s: %w", path, err)
	}
	if err := m.store.BulkSetTranslations(lang, namespace, kv); err != nil {
		return fmt.Errorf("seed %s/%s: %w", lang, namespace, err)
	}
	return nil
}

// seedMetadata accepts either the v1 or v2 translations.meta.json layout and
// upserts all rows into the DB.
func (m *Manager) seedMetadata(data []byte) error {
	// Try v2 (namespaced) first.
	var v2 Metadata
	if err := json.Unmarshal(data, &v2); err == nil && v2.Translations != nil {
		// Heuristic: v2 is a nested map whose leaves look like KeyMetadata.
		// Try to detect by peeking at the first leaf.
		if looksNamespaced(v2.Translations) {
			for lang, namespaces := range v2.Translations {
				for ns, entries := range namespaces {
					dbEntries := make(map[string]db.KeyMetadata, len(entries))
					for key, km := range entries {
						dbEntries[key] = toDBMeta(km)
					}
					if err := m.store.BulkSetMetadata(lang, ns, dbEntries); err != nil {
						return err
					}
				}
			}
			return nil
		}
	}

	// Fall back to v1 layout: { translations: { lang: { key: meta } } }.
	var v1 struct {
		Translations map[string]map[string]KeyMetadata `json:"translations"`
	}
	if err := json.Unmarshal(data, &v1); err != nil {
		return fmt.Errorf("unknown meta format: %w", err)
	}
	for lang, entries := range v1.Translations {
		dbEntries := make(map[string]db.KeyMetadata, len(entries))
		for key, km := range entries {
			dbEntries[key] = toDBMeta(km)
		}
		if err := m.store.BulkSetMetadata(lang, DefaultNamespace, dbEntries); err != nil {
			return err
		}
	}
	return nil
}

// looksNamespaced returns true if the first non-empty leaf of v appears to be
// a map[string]KeyMetadata, i.e. v is lang->ns->key->meta rather than
// lang->key->meta.
func looksNamespaced(v map[string]map[string]map[string]KeyMetadata) bool {
	for _, inner := range v {
		for _, leaf := range inner {
			// If any leaf is a non-empty map then we're indeed v2. An
			// empty-map leaf is ambiguous; keep looking.
			if len(leaf) > 0 {
				return true
			}
		}
		// Outer has at least one namespace-ish key but all its leaves are
		// empty. Treat as v2 anyway — empty namespaces are valid.
		if len(inner) > 0 {
			return true
		}
	}
	return false
}

// reloadCache repopulates the in-memory maps from the DB.
func (m *Manager) reloadCache() error {
	m.mu.Lock()
	defer m.mu.Unlock()

	langs, err := m.store.ListLanguages()
	if err != nil {
		return err
	}

	m.translations = make(map[string]map[string]map[string]string, len(langs))
	m.metadata = make(map[string]map[string]map[string]KeyMetadata, len(langs))

	for _, lang := range langs {
		nsMap, err := m.store.GetAllTranslationsForLang(lang)
		if err != nil {
			return err
		}
		m.translations[lang] = nsMap

		namespaces, err := m.store.ListNamespaces(lang)
		if err != nil {
			return err
		}
		m.metadata[lang] = make(map[string]map[string]KeyMetadata, len(namespaces))
		for _, ns := range namespaces {
			mm, err := m.store.GetAllMetadata(lang, ns)
			if err != nil {
				return err
			}
			converted := make(map[string]KeyMetadata, len(mm))
			for k, v := range mm {
				converted[k] = fromDBMeta(v)
			}
			m.metadata[lang][ns] = converted
		}
	}
	return nil
}

// ---------------------------------------------------------------------------
// JSON export — keeps translations/{lang}/{namespace}.json up to date after
// every write so downstream i18n tooling can keep consuming JSON.
// ---------------------------------------------------------------------------

// exportNamespaceJSON writes translations/{lang}/{namespace}.json for a single
// (lang, namespace) pair.
func (m *Manager) exportNamespaceJSON(lang, namespace string) error {
	m.mu.RLock()
	var snapshot map[string]string
	if nsMap, ok := m.translations[lang]; ok {
		if kv, ok := nsMap[namespace]; ok {
			snapshot = make(map[string]string, len(kv))
			for k, v := range kv {
				snapshot[k] = v
			}
		}
	}
	m.mu.RUnlock()
	if snapshot == nil {
		snapshot = make(map[string]string)
	}

	dir := filepath.Join(m.translationsDir, lang)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	data, err := json.MarshalIndent(snapshot, "", "  ")
	if err != nil {
		return err
	}
	data = append(data, '\n')
	path := filepath.Join(dir, namespace+".json")
	return os.WriteFile(path, data, 0o644)
}

// exportMetadataJSON rewrites translations.meta.json from the full cached
// metadata. Kept because some external tooling still scrapes this file.
func (m *Manager) exportMetadataJSON() error {
	m.mu.RLock()
	out := Metadata{
		Translations: make(map[string]map[string]map[string]KeyMetadata, len(m.metadata)),
	}
	for lang, namespaces := range m.metadata {
		copiedLang := make(map[string]map[string]KeyMetadata, len(namespaces))
		for ns, entries := range namespaces {
			copiedNS := make(map[string]KeyMetadata, len(entries))
			for k, v := range entries {
				copiedNS[k] = v
			}
			copiedLang[ns] = copiedNS
		}
		out.Translations[lang] = copiedLang
	}
	m.mu.RUnlock()

	if err := os.MkdirAll(m.translationsDir, 0o755); err != nil {
		return err
	}
	data, err := json.MarshalIndent(out, "", "  ")
	if err != nil {
		return err
	}
	data = append(data, '\n')
	path := filepath.Join(m.translationsDir, "translations.meta.json")
	return os.WriteFile(path, data, 0o644)
}

// ---------------------------------------------------------------------------
// Read API — served from the in-memory cache.
// ---------------------------------------------------------------------------

// GetLanguages returns all available language codes, sorted.
func (m *Manager) GetLanguages() []string {
	m.mu.RLock()
	defer m.mu.RUnlock()

	langs := make([]string, 0, len(m.translations))
	for lang := range m.translations {
		langs = append(langs, lang)
	}
	sort.Strings(langs)
	return langs
}

// GetNamespaces returns every namespace for a language, sorted. Pass "" to
// get the union across all languages.
func (m *Manager) GetNamespaces(lang string) []string {
	m.mu.RLock()
	defer m.mu.RUnlock()

	seen := make(map[string]struct{})
	if lang == "" {
		for _, nsMap := range m.translations {
			for ns := range nsMap {
				seen[ns] = struct{}{}
			}
		}
	} else if nsMap, ok := m.translations[lang]; ok {
		for ns := range nsMap {
			seen[ns] = struct{}{}
		}
	}
	out := make([]string, 0, len(seen))
	for ns := range seen {
		out = append(out, ns)
	}
	sort.Strings(out)
	return out
}

// GetTranslations returns a copy of key→value for (lang, namespace).
func (m *Manager) GetTranslations(lang, namespace string) map[string]string {
	if namespace == "" {
		namespace = DefaultNamespace
	}

	m.mu.RLock()
	defer m.mu.RUnlock()

	if nsMap, ok := m.translations[lang]; ok {
		if kv, ok := nsMap[namespace]; ok {
			out := make(map[string]string, len(kv))
			for k, v := range kv {
				out[k] = v
			}
			return out
		}
	}
	return make(map[string]string)
}

// TranslationCountForLang returns the total number of translated keys across
// all namespaces for a language.
func (m *Manager) TranslationCountForLang(lang string) int {
	m.mu.RLock()
	defer m.mu.RUnlock()

	total := 0
	if nsMap, ok := m.translations[lang]; ok {
		for _, kv := range nsMap {
			total += len(kv)
		}
	}
	return total
}

// MissingSourceItem represents a key that exists in at least one non-source
// language but is missing from the source language itself. `Source` holds
// the text we sampled from `SourceLang`; together they give the LLM enough
// to back-translate into the source. The Review page uses these to surface
// "you forgot to write the English for this key" cases.
type MissingSourceItem struct {
	Key        string `json:"key"`
	Namespace  string `json:"namespace"`
	Source     string `json:"source"`
	SourceLang string `json:"source_lang"`
}

// GetMissingSourceKeys returns keys missing from (sourceLang, namespace) that
// exist in at least one other language. For each such key it picks the first
// lang (alphabetically, for determinism) that has a non-empty value and uses
// that value as the back-translation sample. Callers get enough info to fire
// `/translate/single` in the reverse direction without another DB round-trip.
func (m *Manager) GetMissingSourceKeys(sourceLang, namespace string) []MissingSourceItem {
	if namespace == "" {
		namespace = DefaultNamespace
	}

	m.mu.RLock()
	defer m.mu.RUnlock()

	var sourceMap map[string]string
	if nsMap, ok := m.translations[sourceLang]; ok {
		sourceMap = nsMap[namespace]
	}

	type cand struct {
		lang   string
		values map[string]string
	}
	cands := make([]cand, 0)
	for lang, nsMap := range m.translations {
		if lang == sourceLang {
			continue
		}
		if km, ok := nsMap[namespace]; ok && len(km) > 0 {
			cands = append(cands, cand{lang: lang, values: km})
		}
	}
	sort.Slice(cands, func(i, j int) bool { return cands[i].lang < cands[j].lang })

	seen := make(map[string]struct{})
	out := make([]MissingSourceItem, 0)
	for _, c := range cands {
		for key, val := range c.values {
			if _, done := seen[key]; done {
				continue
			}
			if _, hasSource := sourceMap[key]; hasSource {
				continue
			}
			if val == "" {
				continue
			}
			seen[key] = struct{}{}
			out = append(out, MissingSourceItem{
				Key:        key,
				Namespace:  namespace,
				Source:     val,
				SourceLang: c.lang,
			})
		}
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Key < out[j].Key })
	return out
}

// GetMissingTranslations returns the keys present in (sourceLang, namespace)
// but missing from (targetLang, namespace).
func (m *Manager) GetMissingTranslations(sourceLang, targetLang, namespace string) []string {
	if namespace == "" {
		namespace = DefaultNamespace
	}

	m.mu.RLock()
	defer m.mu.RUnlock()

	var source map[string]string
	if nsMap, ok := m.translations[sourceLang]; ok {
		source = nsMap[namespace]
	}
	var target map[string]string
	if nsMap, ok := m.translations[targetLang]; ok {
		target = nsMap[namespace]
	}

	var missing []string
	for key := range source {
		if _, ok := target[key]; !ok {
			missing = append(missing, key)
		}
	}
	sort.Strings(missing)
	return missing
}

// GetLowConfidenceTranslations returns keys in (lang, namespace) whose
// metadata confidence is strictly below threshold.
func (m *Manager) GetLowConfidenceTranslations(lang, namespace string, threshold float64) []string {
	if namespace == "" {
		namespace = DefaultNamespace
	}

	m.mu.RLock()
	defer m.mu.RUnlock()

	nsMap, ok := m.metadata[lang]
	if !ok {
		return []string{}
	}
	entries, ok := nsMap[namespace]
	if !ok {
		return []string{}
	}
	var out []string
	for key, meta := range entries {
		if meta.Confidence < threshold {
			out = append(out, key)
		}
	}
	return out
}

// GetMetadata returns metadata for a single key, or (zero, false).
func (m *Manager) GetMetadata(lang, namespace, key string) (KeyMetadata, bool) {
	if namespace == "" {
		namespace = DefaultNamespace
	}

	m.mu.RLock()
	defer m.mu.RUnlock()

	if nsMap, ok := m.metadata[lang]; ok {
		if entries, ok := nsMap[namespace]; ok {
			meta, ok := entries[key]
			return meta, ok
		}
	}
	return KeyMetadata{}, false
}

// GetAllMetadataForNamespace returns a copy of the metadata map for a
// (lang, namespace) pair.
func (m *Manager) GetAllMetadataForNamespace(lang, namespace string) map[string]KeyMetadata {
	if namespace == "" {
		namespace = DefaultNamespace
	}

	m.mu.RLock()
	defer m.mu.RUnlock()

	out := make(map[string]KeyMetadata)
	if nsMap, ok := m.metadata[lang]; ok {
		if entries, ok := nsMap[namespace]; ok {
			for k, v := range entries {
				out[k] = v
			}
		}
	}
	return out
}

// GetLanguagePairStats returns coverage numbers for a (source, target,
// namespace) triple. Only source keys count: a key the target has but the
// source lacks is not a translation of anything yet (it waits for a
// back-fill into the source), so translated + missing == total.
func (m *Manager) GetLanguagePairStats(sourceLang, targetLang, namespace string) map[string]interface{} {
	source := m.GetTranslations(sourceLang, namespace)
	missing := m.GetMissingTranslations(sourceLang, targetLang, namespace)

	totalKeys := len(source)
	missingCount := len(missing)
	translatedKeys := totalKeys - missingCount
	lowConfCount := len(m.GetLowConfidenceTranslations(targetLang, namespace, 0.7))

	var coverage float64
	if totalKeys > 0 {
		coverage = float64(translatedKeys) / float64(totalKeys) * 100
	}

	return map[string]interface{}{
		"total_keys":           totalKeys,
		"translated_keys":      translatedKeys,
		"missing_count":        missingCount,
		"low_confidence_count": lowConfCount,
		"coverage_percentage":  coverage,
		"namespace":            namespace,
	}
}

// ---------------------------------------------------------------------------
// Write API — DB is authoritative; JSON is re-exported after each change.
// ---------------------------------------------------------------------------

// SetTranslation upserts a translation.
func (m *Manager) SetTranslation(lang, namespace, key, value string) error {
	if namespace == "" {
		namespace = DefaultNamespace
	}

	if err := m.store.SetTranslation(lang, namespace, key, value); err != nil {
		return fmt.Errorf("db set translation: %w", err)
	}

	m.mu.Lock()
	if _, ok := m.translations[lang]; !ok {
		m.translations[lang] = make(map[string]map[string]string)
	}
	if _, ok := m.translations[lang][namespace]; !ok {
		m.translations[lang][namespace] = make(map[string]string)
	}
	m.translations[lang][namespace][key] = value
	m.mu.Unlock()

	if err := m.exportNamespaceJSON(lang, namespace); err != nil {
		return fmt.Errorf("export %s/%s.json: %w", lang, namespace, err)
	}
	return nil
}

// SetMetadata upserts metadata.
func (m *Manager) SetMetadata(lang, namespace, key string, meta KeyMetadata) error {
	if namespace == "" {
		namespace = DefaultNamespace
	}

	if err := m.store.SetMetadata(lang, namespace, key, toDBMeta(meta)); err != nil {
		return fmt.Errorf("db set metadata: %w", err)
	}

	m.mu.Lock()
	if _, ok := m.metadata[lang]; !ok {
		m.metadata[lang] = make(map[string]map[string]KeyMetadata)
	}
	if _, ok := m.metadata[lang][namespace]; !ok {
		m.metadata[lang][namespace] = make(map[string]KeyMetadata)
	}
	m.metadata[lang][namespace][key] = meta
	m.mu.Unlock()

	if err := m.exportMetadataJSON(); err != nil {
		return fmt.Errorf("export meta json: %w", err)
	}
	return nil
}

// DeleteKey removes (namespace, key) from every language — translation rows,
// metadata rows, and in-memory caches. The affected namespace JSON files are
// re-exported so the empty {} (or still-populated) state lands on disk, and
// translations.meta.json is refreshed too.
//
// Returns the list of languages that actually had the key. Safe to call on a
// non-existent key (no-op, returns an empty slice).
func (m *Manager) DeleteKey(namespace, key string) ([]string, error) {
	if namespace == "" {
		namespace = DefaultNamespace
	}
	if key == "" {
		return nil, fmt.Errorf("key is required")
	}

	affected, err := m.store.DeleteKeyEverywhere(namespace, key)
	if err != nil {
		return nil, fmt.Errorf("db delete key: %w", err)
	}

	m.mu.Lock()
	affectedLangs := make([]string, 0, len(affected))
	for _, a := range affected {
		affectedLangs = append(affectedLangs, a.Lang)
		if nsMap, ok := m.translations[a.Lang]; ok {
			if kv, ok := nsMap[a.Namespace]; ok {
				delete(kv, key)
			}
		}
		if nsMap, ok := m.metadata[a.Lang]; ok {
			if mm, ok := nsMap[a.Namespace]; ok {
				delete(mm, key)
			}
		}
	}
	m.mu.Unlock()

	var exportErrs []string
	for _, a := range affected {
		if err := m.exportNamespaceJSON(a.Lang, a.Namespace); err != nil {
			exportErrs = append(exportErrs, fmt.Sprintf("%s/%s: %v", a.Lang, a.Namespace, err))
		}
	}
	if err := m.exportMetadataJSON(); err != nil {
		exportErrs = append(exportErrs, fmt.Sprintf("metadata: %v", err))
	}
	if len(exportErrs) > 0 {
		return affectedLangs, fmt.Errorf("key deleted from DB but re-export had issues: %s", strings.Join(exportErrs, "; "))
	}
	return affectedLangs, nil
}

// RenameNamespace moves every key under `oldNS` into `newNS`, across every
// language. The operation is rejected if `newNS` already has any rows (to
// avoid silent bucket merges). The on-disk JSON artefacts are re-exported:
// the new `<lang>/<newNS>.json` files are written, and the old
// `<lang>/<oldNS>.json` files are replaced with an empty `{}` (we don't
// delete them, because another language may still need its sibling file; a
// full `make reset-db` or manual cleanup removes leftover stubs).
func (m *Manager) RenameNamespace(oldNS, newNS string) ([]string, error) {
	oldNS = strings.TrimSpace(oldNS)
	newNS = strings.TrimSpace(newNS)
	if oldNS == "" || newNS == "" {
		return nil, fmt.Errorf("old and new namespace are required")
	}
	if oldNS == newNS {
		return nil, nil
	}

	affected, err := m.store.RenameNamespace(oldNS, newNS)
	if err != nil {
		return nil, fmt.Errorf("db rename namespace: %w", err)
	}

	m.mu.Lock()
	for _, lang := range affected {
		if nsMap, ok := m.translations[lang]; ok {
			if kv, ok := nsMap[oldNS]; ok {
				nsMap[newNS] = kv
				delete(nsMap, oldNS)
			}
		}
		if nsMap, ok := m.metadata[lang]; ok {
			if mm, ok := nsMap[oldNS]; ok {
				nsMap[newNS] = mm
				delete(nsMap, oldNS)
			}
		}
	}
	m.mu.Unlock()

	var exportErrs []string
	for _, lang := range affected {
		if err := m.exportNamespaceJSON(lang, newNS); err != nil {
			exportErrs = append(exportErrs, fmt.Sprintf("%s/%s: %v", lang, newNS, err))
		}
		// Best-effort: remove the old-namespace JSON stub so we don't leave
		// an empty `{}` file lying around for downstream tooling to load.
		oldPath := filepath.Join(m.translationsDir, lang, oldNS+".json")
		if err := os.Remove(oldPath); err != nil && !os.IsNotExist(err) {
			exportErrs = append(exportErrs, fmt.Sprintf("remove %s: %v", oldPath, err))
		}
	}
	if err := m.exportMetadataJSON(); err != nil {
		exportErrs = append(exportErrs, fmt.Sprintf("metadata: %v", err))
	}
	if len(exportErrs) > 0 {
		return affected, fmt.Errorf("namespace renamed in DB but re-export had issues: %s", strings.Join(exportErrs, "; "))
	}
	return affected, nil
}

// DeleteNamespace wipes every key in `ns` across every language, drops the
// in-memory cache entries, and replaces the on-disk JSON files with `{}` so
// downstream tooling sees the bucket as empty. Returns the list of languages
// that actually had rows under the namespace.
func (m *Manager) DeleteNamespace(ns string) ([]string, error) {
	ns = strings.TrimSpace(ns)
	if ns == "" {
		return nil, fmt.Errorf("namespace is required")
	}

	affected, err := m.store.DeleteNamespace(ns)
	if err != nil {
		return nil, fmt.Errorf("db delete namespace: %w", err)
	}

	m.mu.Lock()
	for _, lang := range affected {
		if nsMap, ok := m.translations[lang]; ok {
			delete(nsMap, ns)
		}
		if nsMap, ok := m.metadata[lang]; ok {
			delete(nsMap, ns)
		}
	}
	m.mu.Unlock()

	var exportErrs []string
	for _, lang := range affected {
		// Best-effort: remove the on-disk file so `translations/` stays
		// clean. Empty namespaces that never appear in the DB would leave
		// orphan files otherwise.
		path := filepath.Join(m.translationsDir, lang, ns+".json")
		if err := os.Remove(path); err != nil && !os.IsNotExist(err) {
			exportErrs = append(exportErrs, fmt.Sprintf("remove %s: %v", path, err))
		}
	}
	if err := m.exportMetadataJSON(); err != nil {
		exportErrs = append(exportErrs, fmt.Sprintf("metadata: %v", err))
	}
	if len(exportErrs) > 0 {
		return affected, fmt.Errorf("namespace deleted in DB but cleanup had issues: %s", strings.Join(exportErrs, "; "))
	}
	return affected, nil
}

// NamespaceStats describes how populated a namespace is: the distinct set of
// keys (union across every language) and the number of (lang, key) cells
// that actually have a translation. Used by the "Manage namespaces" UI so
// reviewers can see what they're about to rename / delete.
type NamespaceStats struct {
	Namespace        string `json:"namespace"`
	KeyCount         int    `json:"key_count"`
	TranslationCount int    `json:"translation_count"`
}

// GetNamespaceStats returns per-namespace key + translation counts, summed
// across every language.
func (m *Manager) GetNamespaceStats() []NamespaceStats {
	m.mu.RLock()
	defer m.mu.RUnlock()

	keysByNS := make(map[string]map[string]struct{})
	cellsByNS := make(map[string]int)

	for _, nsMap := range m.translations {
		for ns, kv := range nsMap {
			if _, ok := keysByNS[ns]; !ok {
				keysByNS[ns] = make(map[string]struct{})
			}
			for k, v := range kv {
				keysByNS[ns][k] = struct{}{}
				if v != "" {
					cellsByNS[ns]++
				}
			}
		}
	}

	out := make([]NamespaceStats, 0, len(keysByNS))
	for ns, keys := range keysByNS {
		// Skip empty cache entries left behind after every key in a
		// namespace was deleted — they're a cache artefact, not a real
		// namespace. The "Manage namespaces" UI shouldn't show them.
		if len(keys) == 0 {
			continue
		}
		out = append(out, NamespaceStats{
			Namespace:        ns,
			KeyCount:         len(keys),
			TranslationCount: cellsByNS[ns],
		})
	}
	sort.Slice(out, func(i, j int) bool { return out[i].Namespace < out[j].Namespace })
	return out
}

// DeleteLanguage removes every translation + metadata row for `lang` from the
// DB, drops the in-memory cache for that language, and deletes the on-disk
// JSON artefacts (both the nested `translations/{lang}/` folder and any
// legacy flat `{lang}.json`). Metadata.json is re-exported so downstream
// tools see the language disappear.
func (m *Manager) DeleteLanguage(lang string) error {
	if lang == "" {
		return fmt.Errorf("lang is required")
	}
	if err := m.store.DeleteLanguage(lang); err != nil {
		return fmt.Errorf("db delete language: %w", err)
	}

	m.mu.Lock()
	delete(m.translations, lang)
	delete(m.metadata, lang)
	m.mu.Unlock()

	// Best-effort filesystem cleanup. Failures are logged into the error
	// but callers still see the DB-level delete as authoritative.
	var cleanupErrs []string
	dir := filepath.Join(m.translationsDir, lang)
	if err := os.RemoveAll(dir); err != nil && !os.IsNotExist(err) {
		cleanupErrs = append(cleanupErrs, fmt.Sprintf("remove dir %s: %v", dir, err))
	}
	flat := filepath.Join(m.translationsDir, lang+".json")
	if err := os.Remove(flat); err != nil && !os.IsNotExist(err) {
		cleanupErrs = append(cleanupErrs, fmt.Sprintf("remove %s: %v", flat, err))
	}

	if err := m.exportMetadataJSON(); err != nil {
		cleanupErrs = append(cleanupErrs, fmt.Sprintf("re-export metadata: %v", err))
	}

	if len(cleanupErrs) > 0 {
		return fmt.Errorf("language deleted from DB but cleanup had issues: %s", strings.Join(cleanupErrs, "; "))
	}
	return nil
}

// ---------------------------------------------------------------------------
// Glossary
// ---------------------------------------------------------------------------

// GlossaryEntry is the translation-layer view of a term-base row. The
// presentation shape mirrors db.GlossaryEntry but with JSON tags so it
// serializes nicely in API responses.
type GlossaryEntry struct {
	ID             int64    `json:"id"`
	SourceLang     string   `json:"source_lang"`
	TargetLang     string   `json:"target_lang"`
	SourceTerm     string   `json:"source_term"`
	TargetTerm     string   `json:"target_term"`
	TargetVariants []string `json:"target_variants,omitempty"`
	Note           string   `json:"note"`
	CreatedAt      string   `json:"created_at"`
	UpdatedAt      string   `json:"updated_at"`
}

// ListGlossary returns every entry matching the filters (empty = no filter).
func (m *Manager) ListGlossary(sourceLang, targetLang string) ([]GlossaryEntry, error) {
	rows, err := m.store.ListGlossary(sourceLang, targetLang)
	if err != nil {
		return nil, err
	}
	out := make([]GlossaryEntry, 0, len(rows))
	for _, r := range rows {
		out = append(out, fromDBGlossary(r))
	}
	return out, nil
}

// AddGlossaryEntry validates the payload and inserts a new entry.
func (m *Manager) AddGlossaryEntry(e GlossaryEntry) (GlossaryEntry, error) {
	if err := validateGlossary(e); err != nil {
		return GlossaryEntry{}, err
	}
	id, err := m.store.AddGlossaryEntry(toDBGlossary(e))
	if err != nil {
		return GlossaryEntry{}, err
	}
	e.ID = id
	return e, nil
}

// UpdateGlossaryEntry validates the payload and overwrites an existing entry.
// source_lang and target_lang from the payload are ignored — they're only
// editable via delete-then-add.
func (m *Manager) UpdateGlossaryEntry(id int64, e GlossaryEntry) error {
	if strings.TrimSpace(e.SourceTerm) == "" || strings.TrimSpace(e.TargetTerm) == "" {
		return fmt.Errorf("source_term and target_term are required")
	}
	return m.store.UpdateGlossaryEntry(id, toDBGlossary(e))
}

// DeleteGlossaryEntry removes a single entry by id.
func (m *Manager) DeleteGlossaryEntry(id int64) error {
	return m.store.DeleteGlossaryEntry(id)
}

// DeleteGlossaryEntriesForLang removes every glossary entry involving `lang`
// (as source or target). Used by the DELETE /api/languages handler. Returns
// the number of rows removed.
func (m *Manager) DeleteGlossaryEntriesForLang(lang string) (int64, error) {
	return m.store.DeleteGlossaryEntriesForLang(lang)
}

// GlossaryMatchesForText returns the subset of glossary entries for a (source,
// target) pair whose source_term actually appears in `text` (case-insensitive
// substring match). This is what the translation service passes into the LLM
// prompt — we only send relevant terms rather than the full glossary, to keep
// prompts compact and focus the model.
func (m *Manager) GlossaryMatchesForText(sourceLang, targetLang, text string) ([]GlossaryEntry, error) {
	all, err := m.ListGlossary(sourceLang, targetLang)
	if err != nil || len(all) == 0 {
		return nil, err
	}
	lower := strings.ToLower(text)
	var out []GlossaryEntry
	for _, e := range all {
		term := strings.TrimSpace(e.SourceTerm)
		if term == "" {
			continue
		}
		if strings.Contains(lower, strings.ToLower(term)) {
			out = append(out, e)
		}
	}
	return out, nil
}

func validateGlossary(e GlossaryEntry) error {
	if strings.TrimSpace(e.SourceLang) == "" || strings.TrimSpace(e.TargetLang) == "" {
		return fmt.Errorf("source_lang and target_lang are required")
	}
	if e.SourceLang == e.TargetLang {
		return fmt.Errorf("source_lang and target_lang must differ")
	}
	if strings.TrimSpace(e.SourceTerm) == "" || strings.TrimSpace(e.TargetTerm) == "" {
		return fmt.Errorf("source_term and target_term are required")
	}
	return nil
}

func toDBGlossary(e GlossaryEntry) db.GlossaryEntry {
	return db.GlossaryEntry{
		ID:             e.ID,
		SourceLang:     e.SourceLang,
		TargetLang:     e.TargetLang,
		SourceTerm:     e.SourceTerm,
		TargetTerm:     e.TargetTerm,
		TargetVariants: e.TargetVariants,
		Note:           e.Note,
	}
}
func fromDBGlossary(e db.GlossaryEntry) GlossaryEntry {
	return GlossaryEntry{
		ID:             e.ID,
		SourceLang:     e.SourceLang,
		TargetLang:     e.TargetLang,
		SourceTerm:     e.SourceTerm,
		TargetTerm:     e.TargetTerm,
		TargetVariants: e.TargetVariants,
		Note:           e.Note,
		CreatedAt:      e.CreatedAt,
		UpdatedAt:      e.UpdatedAt,
	}
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

func toDBMeta(m KeyMetadata) db.KeyMetadata {
	return db.KeyMetadata{
		Confidence:   m.Confidence,
		TranslatedBy: m.TranslatedBy,
		Timestamp:    m.Timestamp,
		Context:      m.Context,
	}
}

func fromDBMeta(m db.KeyMetadata) KeyMetadata {
	return KeyMetadata{
		Confidence:   m.Confidence,
		TranslatedBy: m.TranslatedBy,
		Timestamp:    m.Timestamp,
		Context:      m.Context,
	}
}
