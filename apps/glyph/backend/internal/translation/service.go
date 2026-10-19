// Package translation's service layer drives the LLM to fill in missing
// translations. All operations are namespace-aware: a request to translate
// missing keys fills gaps within one (source, target, namespace) triple.
//
// Per-pair LLM routing: each entry in `config.LanguagePairs` names an
// `llm.models[].name`. The service keeps one *llm.Client per configured model
// and picks the right one based on the `{source}_{target}` key. This lets the
// user say "use Viking for sv↔en, use Mistral for en↔fr" in the same app.
package translation

import (
	"fmt"
	"time"

	"glyph/internal/config"
	"glyph/internal/llm"
)

// TranslationService ties the Manager (storage) to one or more LLM clients.
type TranslationService struct {
	manager *Manager
	config  *config.Config

	// clientsByModel maps an `llm.models[].name` to a ready-to-use client.
	// Looked up by name each translation so edits to language_pairs take
	// effect immediately without rebuilding the service.
	clientsByModel map[string]*llm.Client

	// defaultClient is the first configured model, used as a fallback when a
	// pair references a model that doesn't exist (typo, deleted entry, …).
	defaultClient *llm.Client
}

// TranslationResult is returned by both single-key and batch translation.
type TranslationResult struct {
	Key         string  `json:"key"`
	Namespace   string  `json:"namespace"`
	Translation string  `json:"translation"`
	Confidence  float64 `json:"confidence"`
	ModelUsed   string  `json:"model_used"`

	// GlossaryViolations lists preferred-term rules the model ignored for
	// this key. When non-empty, the caller should surface a warning to the
	// reviewer — confidence will also have been capped for these rows.
	GlossaryViolations []GlossaryViolation `json:"glossary_violations,omitempty"`
}

// GlossaryViolation mirrors llm.GlossaryViolation at the service layer so the
// API/UI never reach into the llm package directly.
type GlossaryViolation struct {
	SourceTerm string `json:"source_term"`
	TargetTerm string `json:"target_term"`
}

// NewTranslationService creates a service with one client per configured
// `llm.models[]` entry.
func NewTranslationService(manager *Manager, cfg *config.Config) *TranslationService {
	clients := make(map[string]*llm.Client, len(cfg.LLM.Models))
	var def *llm.Client
	for i, model := range cfg.LLM.Models {
		// Default to chat mode (true) — works for all models on llama-server.
		// Only use raw /completion if explicitly set to false.
		useChatMode := model.ChatMode == nil || *model.ChatMode
		c := llm.NewClient(model.Endpoint, firstNonEmpty(model.Name, model.Model, "llm"), useChatMode)
		clients[model.Name] = c
		if i == 0 {
			def = c
		}
	}

	return &TranslationService{
		manager:        manager,
		config:         cfg,
		clientsByModel: clients,
		defaultClient:  def,
	}
}

// clientForPair returns the LLM client + configured temperature for
// (sourceLang, targetLang). If no pair is configured, or the configured model
// no longer exists, we fall back to the default client and a sane default
// temperature (0.2).
func (ts *TranslationService) clientForPair(sourceLang, targetLang string) (*llm.Client, float64, config.LanguagePair) {
	key := sourceLang + "_" + targetLang
	pair, ok := ts.config.LanguagePairs[key]
	if !ok {
		return ts.defaultClient, 0.2, config.LanguagePair{}
	}
	client, found := ts.clientsByModel[pair.Model]
	if !found {
		client = ts.defaultClient
	}
	return client, pair.ResolvedTemperature(), pair
}

// TranslateMissingKeys translates all missing keys for each target language
// within a single namespace.
func (ts *TranslationService) TranslateMissingKeys(sourceLang string, targetLangs []string, namespace string) map[string]interface{} {
	if namespace == "" {
		namespace = DefaultNamespace
	}
	results := make(map[string]interface{})
	sourceTranslations := ts.manager.GetTranslations(sourceLang, namespace)

	for _, targetLang := range targetLangs {
		missing := ts.manager.GetMissingTranslations(sourceLang, targetLang, namespace)

		translatedCount := 0
		lowConfidenceCount := 0
		translations := []TranslationResult{}

		for _, key := range missing {
			sourceText := sourceTranslations[key]
			if sourceText == "" {
				continue
			}

			result, err := ts.translateSingleWithSourceMeta(sourceLang, targetLang, namespace, key, sourceText)
			if err != nil {
				fmt.Printf("Translation error for %s/%s/%s: %v\n", targetLang, namespace, key, err)
				continue
			}

			translations = append(translations, *result)
			translatedCount++

			threshold := ts.getConfidenceThreshold(sourceLang, targetLang)
			if result.Confidence < threshold {
				lowConfidenceCount++
			}

			if err := ts.manager.SetTranslation(targetLang, namespace, key, result.Translation); err != nil {
				fmt.Printf("Failed to save translation: %v\n", err)
				continue
			}

			existingMeta, _ := ts.manager.GetMetadata(targetLang, namespace, key)
			meta := MergeLLMMetadataStamp(existingMeta, KeyMetadata{
				Confidence:   result.Confidence,
				TranslatedBy: result.ModelUsed,
				Timestamp:    time.Now().UTC().Format(time.RFC3339),
			})
			if err := ts.manager.SetMetadata(targetLang, namespace, key, meta); err != nil {
				fmt.Printf("Failed to save metadata: %v\n", err)
			}
		}

		results[targetLang] = map[string]interface{}{
			"namespace":      namespace,
			"translated":     translatedCount,
			"low_confidence": lowConfidenceCount,
			"missing_count":  len(missing),
			"translations":   translations,
		}
	}

	return results
}

// TranslateSingleKey translates a single (namespace, key) pair.
func (ts *TranslationService) TranslateSingleKey(sourceLang, targetLang, namespace, key, text string) (*TranslationResult, error) {
	if namespace == "" {
		namespace = DefaultNamespace
	}
	return ts.translateSingleWithSourceMeta(sourceLang, targetLang, namespace, key, text)
}

// translateSingleWithSourceMeta loads reviewer context from the source
// language metadata row (if any) and passes it into the LLM prompt.
func (ts *TranslationService) translateSingleWithSourceMeta(sourceLang, targetLang, namespace, key, text string) (*TranslationResult, error) {
	srcMeta, _ := ts.manager.GetMetadata(sourceLang, namespace, key)
	return ts.translateSingle(sourceLang, targetLang, namespace, key, text, srcMeta.Context)
}

// translateSingle is the shared per-key path that picks the right LLM client
// and temperature based on the language pair config, and injects any matching
// glossary entries into the prompt.
func (ts *TranslationService) translateSingle(sourceLang, targetLang, namespace, key, text, usageContext string) (*TranslationResult, error) {
	client, temperature, _ := ts.clientForPair(sourceLang, targetLang)
	if client == nil {
		return nil, fmt.Errorf("no LLM configured (language_pairs[%s_%s] has no usable model)", sourceLang, targetLang)
	}

	sourceDisplay := ts.getLanguageDisplay(sourceLang)
	targetDisplay := ts.getLanguageDisplay(targetLang)

	// Glossary lookup failures are non-fatal — we fall back to translating
	// without a glossary rather than blocking the translation on what is
	// basically a nice-to-have.
	matches, glossaryErr := ts.manager.GlossaryMatchesForText(sourceLang, targetLang, text)
	if glossaryErr != nil {
		fmt.Printf("Glossary lookup error for %s→%s: %v\n", sourceLang, targetLang, glossaryErr)
	}
	terms := make([]llm.GlossaryTerm, 0, len(matches))
	for _, m := range matches {
		terms = append(terms, llm.GlossaryTerm{
			SourceTerm:     m.SourceTerm,
			TargetTerm:     m.TargetTerm,
			TargetVariants: m.TargetVariants,
			Note:           m.Note,
		})
	}

	response, err := client.TranslateWithGlossary(sourceDisplay, targetDisplay, text, temperature, terms, usageContext)
	if err != nil {
		return nil, err
	}

	var violations []GlossaryViolation
	if len(response.GlossaryViolations) > 0 {
		violations = make([]GlossaryViolation, 0, len(response.GlossaryViolations))
		for _, v := range response.GlossaryViolations {
			violations = append(violations, GlossaryViolation{
				SourceTerm: v.SourceTerm,
				TargetTerm: v.TargetTerm,
			})
		}
	}

	return &TranslationResult{
		Key:                key,
		Namespace:          namespace,
		Translation:        response.Text,
		Confidence:         response.Confidence,
		ModelUsed:          client.ModelName(),
		GlossaryViolations: violations,
	}, nil
}

// getConfidenceThreshold returns the threshold for a (source, target) pair.
func (ts *TranslationService) getConfidenceThreshold(sourceLang, targetLang string) float64 {
	key := sourceLang + "_" + targetLang
	if pair, ok := ts.config.LanguagePairs[key]; ok {
		return pair.ConfidenceThreshold
	}
	return 0.7
}

// getLanguageDisplay returns a human-friendly name for a language code.
func (ts *TranslationService) getLanguageDisplay(code string) string {
	languages := map[string]string{
		"en": "English",
		"sv": "Swedish",
		"fr": "French",
		"es": "Spanish",
		"it": "Italian",
		"pt": "Portuguese",
		"nl": "Dutch",
		"pl": "Polish",
		"ru": "Russian",
		"ja": "Japanese",
		"zh": "Chinese",
		"ar": "Arabic",
		"ko": "Korean",
		"fi": "Finnish",
		"no": "Norwegian",
		"da": "Danish",
		"is": "Icelandic",
		"dk": "Danish",
	}
	if display, ok := languages[code]; ok {
		return display
	}
	return code
}

func firstNonEmpty(values ...string) string {
	for _, v := range values {
		if v != "" {
			return v
		}
	}
	return ""
}

// SuggestGlossaryVariants asks the LLM configured for the (source, target)
// pair to list common grammatical forms of `targetTerm`. Used by the
// glossary editor's "Find variants" button so reviewers don't have to hand-
// type every inflection of a preferred term.
//
// Returns the model's best-effort list (or an empty slice if nothing came
// back); the UI layers dedupe + merge the suggestions into whatever the
// reviewer already has typed.
func (ts *TranslationService) SuggestGlossaryVariants(sourceLang, targetLang, sourceTerm, targetTerm string) ([]string, error) {
	client, _, _ := ts.clientForPair(sourceLang, targetLang)
	if client == nil {
		return nil, fmt.Errorf("no LLM configured (language_pairs[%s_%s] has no usable model)", sourceLang, targetLang)
	}
	sourceDisplay := ts.getLanguageDisplay(sourceLang)
	targetDisplay := ts.getLanguageDisplay(targetLang)
	return client.SuggestVariants(sourceDisplay, targetDisplay, sourceTerm, targetTerm)
}
