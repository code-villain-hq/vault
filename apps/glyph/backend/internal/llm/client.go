// Package llm is a thin HTTP client for any llama.cpp-compatible completion
// server (llama-server, text-generation-webui, vLLM's llama-server mode, …).
//
// The client is intentionally minimal: it builds a translation prompt, POSTs
// to `{endpoint}/completion`, and extracts a JSON response of the form
// `{"translation": "...", "confidence": 0.95}` that the prompt instructs the
// model to emit.
package llm

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"math"
	"net"
	"net/http"
	"net/url"
	"regexp"
	"strings"
	"time"
)

// translationJSONObjectStart finds JSON objects that declare a "translation"
// field. We scan every match and json-decode from that offset so only the
// first complete value is consumed — this survives models that echo the
// prompt, repeat "Text to translate / JSON Response" blocks, or emit multiple
// objects; we keep the last successfully decoded object (usually the final
// answer). The old "first { to last }" slice often produced invalid JSON and
// fell back to returning the entire model output as the translation string.
var translationJSONObjectStart = regexp.MustCompile(`\{\s*"translation"\s*:`)

var variantsJSONObjectStart = regexp.MustCompile(`\{\s*"variants"\s*:`)

// llama.cpp can take well over 30s per completion on larger quantized models
// (especially with glossary retries). A short client timeout surfaces as 500
// "context deadline exceeded" from the translation API.
const llmHTTPClientTimeout = 10 * time.Minute

// nPredict caps generation length for /completion. Translation strings are
// short — 256 tokens is generous. The old value of 2048 let the model loop
// endlessly when it failed to stop.
const llmCompletionNPredict = 256

// NormalizeLLMEndpoint rewrites "localhost" to 127.0.0.1 so outbound HTTP
// uses IPv4 loopback. On many macOS setups "localhost" resolves to [::1]
// first while llama-server listens only on 127.0.0.1 — then dials fail with
// "dial tcp [::1]:PORT: connect: connection refused".
func NormalizeLLMEndpoint(endpoint string) string {
	s := strings.TrimSpace(strings.TrimRight(endpoint, "/"))
	u, err := url.Parse(s)
	if err != nil || u.Host == "" {
		return s
	}
	if !strings.EqualFold(u.Hostname(), "localhost") {
		return s
	}
	port := u.Port()
	if port == "" {
		switch u.Scheme {
		case "https", "wss":
			port = "443"
		default:
			port = "80"
		}
	}
	u.Host = net.JoinHostPort("127.0.0.1", port)
	return strings.TrimRight(u.String(), "/")
}

// Client is a single configured backend (one endpoint + a logical model name).
// Clients are cheap; the TranslationService holds one per configured
// `llm.models[]` entry so each language-pair can pick the right one.
type Client struct {
	endpoint   string
	modelName  string
	chatMode   bool
	httpClient *http.Client
}

// TranslationRequest is the logical request shape. We don't actually send this
// as JSON to the LLM — it's here as API documentation for the service layer.
type TranslationRequest struct {
	SourceLang  string  `json:"source_lang"`
	TargetLang  string  `json:"target_lang"`
	Text        string  `json:"text"`
	Temperature float64 `json:"temperature"`
}

// TranslationResponse is the extracted result returned to the service layer.
type TranslationResponse struct {
	Text       string  `json:"text"`
	Confidence float64 `json:"confidence"`

	// GlossaryViolations lists every glossary term that was in scope for this
	// translation but whose target_term did NOT appear (case-insensitively) in
	// the model's output. Populated by TranslateWithGlossary; nil when no
	// glossary was passed.
	GlossaryViolations []GlossaryViolation `json:"glossary_violations,omitempty"`
}

// GlossaryTerm is a preferred-term rule injected into the translation prompt
// to keep the LLM from drifting across synonyms. Use sparingly — the whole
// list is serialized into every prompt for matching source text.
//
// TargetVariants are optional additional accepted forms of TargetTerm. The
// validator treats any of them as equivalent to the canonical TargetTerm —
// this is how we let Swedish definite "e-posten" count as a valid rendering
// of the canonical "e-post" without having to either (a) bake per-language
// morphology into the validator or (b) ask the model to refuse grammatical
// inflections. The variants list is surfaced to the LLM in the prompt so it
// knows which inflections are actually acceptable.
type GlossaryTerm struct {
	SourceTerm     string
	TargetTerm     string
	TargetVariants []string
	Note           string
}

// GlossaryViolation records that the model's output ignored a preferred-term
// rule — typically because it substituted a synonym or a morphological
// variant (e.g. "e-postadressen" instead of "e-post" in Swedish).
type GlossaryViolation struct {
	SourceTerm string `json:"source_term"`
	TargetTerm string `json:"target_term"`
}

// NewClient creates a new LLM client bound to an endpoint and a logical model
// name. The model name is purely cosmetic — it's stamped into metadata rows
// so users know which backend produced a given translation.
// When chatMode is true, the client uses /v1/chat/completions (OpenAI chat
// format) instead of /completion (llama.cpp raw completion).
func NewClient(endpoint, modelName string, chatMode bool) *Client {
	return &Client{
		endpoint:  NormalizeLLMEndpoint(endpoint),
		modelName: modelName,
		chatMode:  chatMode,
		httpClient: &http.Client{
			Timeout: llmHTTPClientTimeout,
		},
	}
}

// ModelName returns the logical model name this client was constructed with.
func (c *Client) ModelName() string {
	return c.modelName
}

// Endpoint returns the HTTP base URL this client talks to.
func (c *Client) Endpoint() string {
	return c.endpoint
}

// Translate is the default entrypoint, using the built-in default temperature
// (0.2). Kept for callers that don't care about sampling parameters.
func (c *Client) Translate(sourceLang, targetLang, text string) (*TranslationResponse, error) {
	return c.TranslateWithOptions(sourceLang, targetLang, text, 0.2)
}

// TranslateWithOptions is the per-pair entrypoint: callers pass the
// configured temperature for this language direction. Any value ≤ 0 is
// coerced to 0.2 to avoid degenerate/looping outputs from some models.
func (c *Client) TranslateWithOptions(sourceLang, targetLang, text string, temperature float64) (*TranslationResponse, error) {
	return c.TranslateWithGlossary(sourceLang, targetLang, text, temperature, nil, "")
}

// maxGlossaryRetries caps how many extra LLM round-trips we burn trying to
// nudge the model into using the exact glossary term. Two retries (three
// attempts total) is the sweet spot — beyond that the model almost never
// recovers, and the latency cost of polling a 7B model three times already
// pushes a single cell to the ~4s ballpark.
const maxGlossaryRetries = 2

// TranslateWithGlossary is the full-featured entrypoint: same as
// TranslateWithOptions, but also lets callers attach a list of preferred
// terms. Empty/nil glossary is equivalent to TranslateWithOptions.
//
// Glossary enforcement is a loop, not a one-shot: if the first attempt
// violates a rule (e.g. the model returns "e-postadressen" when the glossary
// says the target must be exactly "e-post"), we build a correction prompt
// that shows the model its own bad output and ask it to redo the translation
// with the term verbatim. Up to `maxGlossaryRetries` follow-ups are issued.
// The first attempt that honours all glossary rules wins; if none do, the
// last attempt is returned with `GlossaryViolations` populated so the UI can
// still flag it (and cap confidence below the review threshold).
func (c *Client) TranslateWithGlossary(sourceLang, targetLang, text string, temperature float64, glossary []GlossaryTerm, usageContext string) (*TranslationResponse, error) {
	if temperature <= 0 {
		temperature = 0.2
	}
	if temperature > 2 {
		temperature = 2
	}

	var (
		translation string
		confidence  float64
		violations  []GlossaryViolation
	)

	prompt := c.buildPrompt(sourceLang, targetLang, text, glossary, usageContext)
	for attempt := 0; attempt <= maxGlossaryRetries; attempt++ {
		responseText, err := c.callLLM(prompt, temperature)
		if err != nil {
			return nil, err
		}
		log.Printf("[LLM] Raw response (attempt %d): %q", attempt, responseText)
		translation, confidence = c.parseResponse(responseText)
		phOK := mustachePlaceholdersPreserved(text, translation)
		violations = checkGlossaryViolations(translation, glossary)

		// Best case: template tokens ({{…}}) intact and glossary clean.
		if phOK && len(violations) == 0 {
			return &TranslationResponse{
				Text:       translation,
				Confidence: confidence,
			}, nil
		}

		// Last attempt — no point in building another correction prompt.
		if attempt == maxGlossaryRetries {
			break
		}

		// Fix dropped or mangled {{…}} placeholders before glossary issues —
		// a wrong token breaks runtime rendering in every language.
		if !phOK {
			prompt = buildPlaceholderCorrectionPrompt(sourceLang, targetLang, text, translation, usageContext, c.chatMode)
			continue
		}

		// Glossary-only correction path.
		prompt = c.buildCorrectionPrompt(sourceLang, targetLang, text, translation, violations, glossary, usageContext)
	}

	// All retries exhausted and still violating. Surface it to the caller so
	// the UI can cap confidence + display the amber chip. We deliberately do
	// NOT show a user-visible error here — the goal of the retry loop is to
	// make violations invisible when we succeed, not to nag louder when we
	// fail. The amber "needs review" chip is enough signal.
	phOK := mustachePlaceholdersPreserved(text, translation)
	violations = checkGlossaryViolations(translation, glossary)
	if confidence > 0.5 && (!phOK || len(violations) > 0) {
		confidence = 0.5
	}
	return &TranslationResponse{
		Text:               translation,
		Confidence:         confidence,
		GlossaryViolations: violations,
	}, nil
}

// callLLM issues a single prompt to the configured endpoint and returns the
// raw response text. Uses /v1/chat/completions (OpenAI chat format) by
// default — llama-server exposes this for ALL models and it ensures
// instruct/chat models get proper role tokens. Falls back to /completion
// only if chat_mode is explicitly disabled in config.
func (c *Client) callLLM(prompt string, temperature float64) (string, error) {
	if c.chatMode {
		return c.callChatLLM(prompt, temperature)
	}
	return c.callCompletionLLM(prompt, temperature)
}

// callCompletionLLM uses llama-server's /completion endpoint (raw text mode).
// Used when chat_mode is explicitly set to false in config.
func (c *Client) callCompletionLLM(prompt string, temperature float64) (string, error) {
	reqBody := map[string]interface{}{
		"prompt":      prompt,
		"stream":      false,
		"temperature": temperature,
		"n_predict":   llmCompletionNPredict,
		"stop":        []string{"###", "Source:", "Translate from", "Text to translate:"},
	}

	jsonData, err := json.Marshal(reqBody)
	if err != nil {
		return "", fmt.Errorf("failed to marshal request: %w", err)
	}

	url := fmt.Sprintf("%s/completion", c.endpoint)
	resp, err := c.httpClient.Post(url, "application/json", bytes.NewBuffer(jsonData))
	if err != nil {
		return "", fmt.Errorf("failed to call LLM: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return "", fmt.Errorf("LLM returned status %d: %s", resp.StatusCode, string(body))
	}

	var llmResp map[string]interface{}
	if err := json.NewDecoder(resp.Body).Decode(&llmResp); err != nil {
		return "", fmt.Errorf("failed to parse LLM response: %w", err)
	}

	return extractGeneratedText(llmResp)
}

// callChatLLM uses the OpenAI-compatible /v1/chat/completions endpoint.
// llama-server exposes this for ALL models (it applies the model's chat
// template automatically). Chat/instruct models get proper role tokens;
// base models get a sensible default template.
func (c *Client) callChatLLM(prompt string, temperature float64) (string, error) {
	// Be very explicit - small models like EuroLLM 1.7B need clear instructions.
	// Both system and user message contain the format requirement.
	reqBody := map[string]interface{}{
		"model":       c.modelName,
		"stream":      false,
		"temperature": temperature,
		"max_tokens":  llmCompletionNPredict,
		"messages": []map[string]string{
			{"role": "system", "content": "You are a translator. Translate the user's text. Output ONLY JSON: {\"translation\": \"...\", \"confidence\": 0.95}. No other text."},
			{"role": "user", "content": prompt + "\n\nRespond with ONLY JSON. No explanation."},
		},
	}

	jsonData, err := json.Marshal(reqBody)
	if err != nil {
		return "", fmt.Errorf("failed to marshal chat request: %w", err)
	}

	url := fmt.Sprintf("%s/v1/chat/completions", c.endpoint)
	resp, err := c.httpClient.Post(url, "application/json", bytes.NewBuffer(jsonData))
	if err != nil {
		return "", fmt.Errorf("failed to call LLM (chat): %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		body, _ := io.ReadAll(resp.Body)
		return "", fmt.Errorf("LLM (chat) returned status %d: %s", resp.StatusCode, string(body))
	}

	var llmResp map[string]interface{}
	if err := json.NewDecoder(resp.Body).Decode(&llmResp); err != nil {
		return "", fmt.Errorf("failed to parse LLM chat response: %w", err)
	}

	return extractGeneratedText(llmResp)
}

// extractGeneratedText pulls the assistant/completion string from a
// llama-server /completion JSON body. Supports the native shape (`content`,
// `response`, `text`) and OpenAI-compat envelopes (`choices[].text`,
// `choices[].message.content`) used by some builds and proxies.
func extractGeneratedText(llmResp map[string]interface{}) (string, error) {
	if s, ok := llmResp["content"].(string); ok {
		return s, nil
	}
	if s, ok := llmResp["response"].(string); ok {
		return s, nil
	}
	if s, ok := llmResp["text"].(string); ok {
		return s, nil
	}
	chRaw, ok := llmResp["choices"].([]interface{})
	if !ok || len(chRaw) == 0 {
		return "", fmt.Errorf("invalid response format from LLM")
	}
	ch0, ok := chRaw[0].(map[string]interface{})
	if !ok {
		return "", fmt.Errorf("invalid response format from LLM")
	}
	if s, ok := ch0["text"].(string); ok {
		return s, nil
	}
	if msg, ok := ch0["message"].(map[string]interface{}); ok {
		if s, ok := msg["content"].(string); ok {
			return s, nil
		}
	}
	return "", fmt.Errorf("invalid response format from LLM")
}

// buildPrompt creates a prompt for translation. When `glossary` is non-empty,
// a "Glossary" section is injected right before the translation request so
// the model treats those mappings as hard rules — this is how we enforce
// things like "email → e-post" in Swedish no matter what the LLM prefers.
//
// If a term has TargetVariants, those are listed alongside the canonical
// target so the model understands that grammatical inflections (e.g. Swedish
// definite "e-posten") are acceptable but foreign compounds like
// "e-postadressen" are not.
func usageContextBlock(usageContext string) string {
	usageContext = strings.TrimSpace(usageContext)
	if usageContext == "" {
		return ""
	}
	return fmt.Sprintf(`
Where this string appears in the product (use for tone, register, and disambiguation — do not translate this paragraph literally into the output):
%s
`, usageContext)
}

func (c *Client) buildPrompt(sourceLang, targetLang, text string, glossary []GlossaryTerm, usageContext string) string {
	var glossaryBlock string
	if len(glossary) > 0 {
		var sb bytes.Buffer
		sb.WriteString("\nGlossary — MANDATORY preferred terms. Each source term MUST be rendered as one of the accepted forms listed below.\n")
		sb.WriteString("Rules:\n")
		sb.WriteString("  * Use ONLY the accepted forms listed for each mapping. When a list of accepted forms is given, any one of them is fine — pick whichever fits the sentence grammar.\n")
		sb.WriteString("  * Do NOT embed the target term inside a different compound word or add a foreign prefix/suffix (e.g. in Swedish, output \"e-post\" or \"e-posten\", never \"e-postadressen\" or \"e-postmeddelande\").\n")
		sb.WriteString("  * If a rewording is needed to fit the sentence, reword the rest of the sentence — never the glossary term.\n")
		sb.WriteString("Mappings:\n")
		for _, g := range glossary {
			// Note is intentionally stripped — it's reviewer-facing context,
			// not something the model should see in the prompt.
			if len(g.TargetVariants) > 0 {
				fmt.Fprintf(&sb, "  - %q → %q (accepted forms: %s)\n",
					g.SourceTerm, g.TargetTerm, formatAcceptedForms(g.TargetTerm, g.TargetVariants))
			} else {
				fmt.Fprintf(&sb, "  - %q → %q\n", g.SourceTerm, g.TargetTerm)
			}
		}
		glossaryBlock = sb.String()
	}

	ctxBlock := usageContextBlock(usageContext)

	// Chat mode: system message already tells the model to reply with JSON.
	if c.chatMode {
		return fmt.Sprintf("Translate from %s to %s: %q", sourceLang, targetLang, text)
	}

	// Completion mode — include full instructions since there's no system message.
	return fmt.Sprintf(`You are a translation engine. Translate the following text from %s to %s.
Reply ONLY with a JSON object with two fields: "translation" (the translated text) and "confidence" (a number between 0 and 1).
%s%s%s
Source text: %q

JSON:`, sourceLang, targetLang, mustachePreserveInstructions, ctxBlock, glossaryBlock, text)
}

// buildCorrectionPrompt builds a second-shot prompt the retry loop uses when
// the first attempt violates one or more glossary rules. Showing the model
// its own bad output + the specific rules it broke empirically recovers far
// better than generic "please try again" phrasing — the model treats it as
// a proofreading task rather than a fresh translation.
func (c *Client) buildCorrectionPrompt(sourceLang, targetLang, text, previousTranslation string, violations []GlossaryViolation, glossary []GlossaryTerm, usageContext string) string {
	var sb bytes.Buffer
	sb.WriteString(fmt.Sprintf("You are a professional translator. Your previous translation from %s to %s broke glossary rules and must be corrected.\n\n", sourceLang, targetLang))
	sb.WriteString("Preserve every {{ ... }} placeholder from the source text EXACTLY (byte-identical) in your corrected output — translate only the natural-language parts outside those tokens.\n\n")
	if ub := usageContextBlock(usageContext); ub != "" {
		sb.WriteString(strings.TrimSpace(ub))
		sb.WriteString("\n\n")
	}
	fmt.Fprintf(&sb, "Source text: %q\n", text)
	fmt.Fprintf(&sb, "Your previous (INCORRECT) translation: %q\n\n", previousTranslation)

	// Map source→accepted-forms so the correction message lists every
	// acceptable rendering for each violated rule, not just the canonical
	// one. Reduces the chance the model flips from "wrong compound" to
	// "right stem but wrong inflection" on the next attempt.
	acceptedBySource := make(map[string][]string, len(glossary))
	for _, g := range glossary {
		acceptedBySource[g.SourceTerm] = append([]string{g.TargetTerm}, g.TargetVariants...)
	}

	sb.WriteString("The previous translation is INCORRECT because:\n")
	for _, v := range violations {
		forms := acceptedBySource[v.SourceTerm]
		if len(forms) <= 1 {
			fmt.Fprintf(&sb, "  * %q must be translated as EXACTLY %q — not inside a compound word, not inflected, not pluralized, not with any prefix or suffix.\n",
				v.SourceTerm, v.TargetTerm)
		} else {
			fmt.Fprintf(&sb, "  * %q must be translated using one of these accepted forms: %s. Do NOT embed it in a different compound word (e.g. no \"-adressen\" suffix) and do NOT invent new inflections.\n",
				v.SourceTerm, formatAcceptedForms(v.TargetTerm, acceptedBySource[v.SourceTerm][1:]))
		}
	}
	sb.WriteString("\nRewrite the translation so every glossary term above appears as one of its accepted forms, as a standalone word. If the sentence structure needs to change to make that work, change it. Do not include any explanation.\n\n")

	// Include the full glossary block again so the model has the canonical
	// mapping right above the final instruction — helps reduce drift on
	// models with short attention spans.
	if len(glossary) > 0 {
		sb.WriteString("Full glossary (use one of the accepted forms for each mapping):\n")
		for _, g := range glossary {
			if len(g.TargetVariants) > 0 {
				fmt.Fprintf(&sb, "  - %q → %q (accepted forms: %s)\n",
					g.SourceTerm, g.TargetTerm, formatAcceptedForms(g.TargetTerm, g.TargetVariants))
			} else {
				fmt.Fprintf(&sb, "  - %q → %q\n", g.SourceTerm, g.TargetTerm)
			}
		}
		sb.WriteString("\n")
	}

	sb.WriteString("Correct the translation by following these rules:\n")
	for _, v := range violations {
		forms := acceptedBySource[v.SourceTerm]
		if len(forms) <= 1 {
			fmt.Fprintf(&sb, "  - %q MUST be %q (not in a compound word)\n", v.SourceTerm, v.TargetTerm)
		} else {
			fmt.Fprintf(&sb, "  - %q MUST use one of: %s\n", v.SourceTerm, formatAcceptedForms(v.TargetTerm, acceptedBySource[v.SourceTerm][1:]))
		}
	}

	// In chat mode the system message handles JSON formatting.
	// In completion mode we must include it inline.
	if c.chatMode {
		sb.WriteString("\nProvide the corrected translation.")
	} else {
		sb.WriteString("\nReply ONLY with a JSON object with two fields: \"translation\" (the corrected text) and \"confidence\" (a number between 0 and 1).\n\nJSON:")
	}
	return sb.String()
}

// checkGlossaryViolations returns every glossary entry where NONE of the
// accepted forms (the canonical target_term plus any target_variants) is
// present in the translated output as an isolated token — i.e. not glued to
// surrounding letters/digits that would make it part of a compound word. An
// empty/nil glossary returns nil.
//
// The check is deliberately lenient on punctuation and casing, and tolerant
// of declensions that the reviewer has explicitly whitelisted via variants
// (e.g. Swedish definite "e-posten" for the canonical "e-post"). It remains
// strict on compounds: "e-postadressen" never matches "e-post", regardless
// of variants, because of the trailing letter after the needle.
func checkGlossaryViolations(translation string, glossary []GlossaryTerm) []GlossaryViolation {
	if len(glossary) == 0 || translation == "" {
		return nil
	}
	lower := strings.ToLower(translation)
	var out []GlossaryViolation
	for _, g := range glossary {
		if anyAcceptedFormPresent(lower, g) {
			continue
		}
		// Skip entries where every accepted form is blank — a degenerate
		// rule that would otherwise flag every translation.
		target := strings.ToLower(strings.TrimSpace(g.TargetTerm))
		if target == "" && !anyNonEmpty(g.TargetVariants) {
			continue
		}
		out = append(out, GlossaryViolation{
			SourceTerm: g.SourceTerm,
			TargetTerm: g.TargetTerm,
		})
	}
	return out
}

// anyAcceptedFormPresent reports whether the haystack (already lowercased)
// contains the canonical target term OR any of the listed variants as an
// isolated word. Blank entries are skipped.
func anyAcceptedFormPresent(lowerHaystack string, g GlossaryTerm) bool {
	if target := strings.ToLower(strings.TrimSpace(g.TargetTerm)); target != "" {
		if containsAsWord(lowerHaystack, target) {
			return true
		}
	}
	for _, v := range g.TargetVariants {
		variant := strings.ToLower(strings.TrimSpace(v))
		if variant == "" {
			continue
		}
		if containsAsWord(lowerHaystack, variant) {
			return true
		}
	}
	return false
}

// anyNonEmpty returns true if at least one string in the slice is non-blank
// after trimming whitespace. Used to tell "rule with only variants set" apart
// from "fully blank, skip this rule".
func anyNonEmpty(ss []string) bool {
	for _, s := range ss {
		if strings.TrimSpace(s) != "" {
			return true
		}
	}
	return false
}

// formatAcceptedForms renders a human- and LLM-friendly comma-separated list
// of accepted forms starting with the canonical `target`, followed by every
// non-blank variant. Blanks are dropped so duplicates don't sneak into the
// prompt when the variants list happens to echo the canonical form.
func formatAcceptedForms(target string, variants []string) string {
	seen := map[string]struct{}{}
	var out []string
	push := func(s string) {
		t := strings.TrimSpace(s)
		if t == "" {
			return
		}
		k := strings.ToLower(t)
		if _, ok := seen[k]; ok {
			return
		}
		seen[k] = struct{}{}
		out = append(out, fmt.Sprintf("%q", t))
	}
	push(target)
	for _, v := range variants {
		push(v)
	}
	return strings.Join(out, ", ")
}

// containsAsWord returns true if `needle` appears in `haystack` with word
// boundaries on both sides. "Word" here means "letter or digit in any
// script"; hyphens are treated as boundaries so "e-post" in "e-postadressen"
// is NOT a match (there's a trailing "a"), while "e-post" in "nya e-post."
// IS a match (trailing ".").
func containsAsWord(haystack, needle string) bool {
	if needle == "" {
		return false
	}
	idx := 0
	for {
		rel := strings.Index(haystack[idx:], needle)
		if rel < 0 {
			return false
		}
		start := idx + rel
		end := start + len(needle)
		if isWordBoundary(haystack, start, -1) && isWordBoundary(haystack, end, +1) {
			return true
		}
		idx = start + 1
	}
}

// isWordBoundary reports whether the character at `pos` (looking in the
// `direction`: -1 = the char before `pos`, +1 = the char at `pos`) is a
// boundary — i.e. either off the end of the string or a non-letter/digit.
func isWordBoundary(s string, pos, direction int) bool {
	if direction < 0 {
		if pos <= 0 {
			return true
		}
		return !isLetterOrDigit(s[pos-1])
	}
	if pos >= len(s) {
		return true
	}
	return !isLetterOrDigit(s[pos])
}

func isLetterOrDigit(b byte) bool {
	// Intentionally conservative: ASCII letters/digits are always "inside a
	// word"; high-bit bytes (multi-byte UTF-8 runes like "å", "ä", "ö") are
	// also treated as inside a word so "e-post" doesn't match "e-postå..."
	// through a naive boundary check.
	switch {
	case b >= '0' && b <= '9':
		return true
	case b >= 'a' && b <= 'z':
		return true
	case b >= 'A' && b <= 'Z':
		return true
	case b >= 0x80:
		return true
	}
	return false
}

// SuggestVariants asks the model for a short list of grammatical forms /
// accepted inflections of `targetTerm` in `targetLang` — definite forms,
// plurals, case-marked variants, common contractions, etc. The response is
// parsed into a deduped slice of strings; the canonical `targetTerm` itself
// is filtered out so callers can merge the suggestions directly into a
// `target_variants` list without introducing a self-reference.
//
// Returns an empty slice (not an error) when the model declines or emits
// unparseable output — the "Find variants" UI treats that as "LLM had
// nothing useful; reviewer can type them manually".
func (c *Client) SuggestVariants(sourceLang, targetLang, sourceTerm, targetTerm string) ([]string, error) {
	prompt := buildSuggestVariantsPrompt(sourceLang, targetLang, sourceTerm, targetTerm)
	// Low temperature: we want deterministic, obvious inflections, not
	// creative synonyms. If the model wants to hedge we'd rather it return
	// fewer items than more.
	responseText, err := c.callLLM(prompt, 0.1)
	if err != nil {
		return nil, err
	}
	return parseVariantsResponse(responseText, targetTerm), nil
}

// buildSuggestVariantsPrompt constructs the "list accepted grammatical forms"
// prompt. It's deliberately narrow: we want inflections, not synonyms, and
// absolutely no compounds — "e-postadressen" must not sneak back in as a
// suggested variant of "e-post".
func buildSuggestVariantsPrompt(sourceLang, targetLang, sourceTerm, targetTerm string) string {
	return fmt.Sprintf(`You are a %s linguist. The word "%s" is the canonical %s translation of the %s term "%s".

List every common grammatical form of "%s" that a reviewer should accept as equivalent in translations — e.g. definite, plural, genitive, or case-marked forms.

STRICT RULES:
  * ONLY grammatical inflections of "%s". No synonyms.
  * NEVER embed "%s" inside a different compound word (e.g. in Swedish do NOT include "e-postadressen" or "e-postmeddelande" as forms of "e-post").
  * If the term contains Mustache-style "{{" "}}" segments, copy those segments verbatim in every variant — never translate or edit text inside double braces.
  * Include the canonical form "%s" itself if it is a normal surface form.
  * Return at most 6 forms. If the language has little or no inflection, return fewer.

Respond ONLY with valid JSON in this exact format:
{
  "variants": ["form1", "form2", "form3"]
}

JSON Response:`,
		targetLang, targetTerm, targetLang, sourceLang, sourceTerm,
		targetTerm, targetTerm, targetTerm, targetTerm)
}

// filterVariantList dedupes variant strings and drops the canonical target.
func filterVariantList(parsed []string, canonicalTarget string) []string {
	canonical := strings.ToLower(strings.TrimSpace(canonicalTarget))
	seen := map[string]struct{}{}
	out := make([]string, 0, len(parsed))
	for _, v := range parsed {
		t := strings.TrimSpace(v)
		if t == "" {
			continue
		}
		k := strings.ToLower(t)
		if k == canonical {
			continue
		}
		if _, dup := seen[k]; dup {
			continue
		}
		seen[k] = struct{}{}
		out = append(out, t)
	}
	return out
}

// parseVariantsResponse pulls the variants list out of the model's JSON
// response. Lenient: extra prose around the JSON is OK, a list-of-strings
// shape is required. The canonical target term is filtered out so the
// caller's final `target_variants` slice doesn't duplicate it.
func parseVariantsResponse(response, canonicalTarget string) []string {
	data := []byte(response)
	var variants []string
	found := false
	for _, loc := range variantsJSONObjectStart.FindAllIndex(data, -1) {
		idx := loc[0]
		dec := json.NewDecoder(bytes.NewReader(data[idx:]))
		var parsed struct {
			Variants []string `json:"variants"`
		}
		if err := dec.Decode(&parsed); err != nil {
			continue
		}
		variants = parsed.Variants
		found = true
	}
	if !found {
		startIdx := strings.Index(response, "{")
		endIdx := strings.LastIndex(response, "}")
		if startIdx < 0 || endIdx < 0 || startIdx >= endIdx {
			return nil
		}
		var parsed struct {
			Variants []string `json:"variants"`
		}
		if err := json.Unmarshal([]byte(response[startIdx:endIdx+1]), &parsed); err != nil {
			return nil
		}
		variants = parsed.Variants
	}
	return filterVariantList(variants, canonicalTarget)
}

func confidenceFromInterface(v interface{}) float64 {
	conf := func() float64 {
		switch x := v.(type) {
		case float64:
			return x
		case json.Number:
			f, _ := x.Float64()
			return f
		default:
			return 0
		}
	}()

	// Normalize out-of-range values (e.g., 95 → 0.95)
	if conf > 1 {
		conf = conf / 100
	}
	if conf < 0 {
		conf = 0
	}

	// Round to 2 decimal places for cleaner display
	conf = math.Round(conf*100) / 100
	if conf > 1 {
		conf = 1
	}

	return conf
}

// parseResponse extracts translation and confidence from the LLM's JSON-ish
// output. Prefers the last JSON object that decodes as {"translation":...};
// falls back to a single outermost {...} slice for backwards compatibility.
// As a last resort, extracts free-text translations from common model output
// patterns (e.g. "### Translation:\n…") so that models which ignore the JSON
// parseResponse extracts translation text from the model's response.
// Strategies (tried in order):
//  1. JSON objects with "translation" field (legacy models that still emit JSON)
//  2. Outermost { … } JSON fallback
//  3. Pipe-separated "translation | confidence" format (current prompt format)
//  4. Free-text "Translation:" label
//  5. Plain text — first meaningful line
func (c *Client) parseResponse(response string) (string, float64) {
	data := []byte(response)

	// Strategy 1: explicit {"translation": "…"} JSON objects.
	var (
		lastTranslation string
		lastConfidence  float64
		found           bool
	)
	for _, loc := range translationJSONObjectStart.FindAllIndex(data, -1) {
		idx := loc[0]
		dec := json.NewDecoder(bytes.NewReader(data[idx:]))
		var result map[string]interface{}
		if err := dec.Decode(&result); err != nil {
			continue
		}
		tr, ok := result["translation"].(string)
		if !ok || isPlaceholder(tr) {
			continue
		}
		// Strip surrounding quotes. Handle "foo", \"foo\", and "foo" (trailing only)
		tr = strings.Trim(tr, `"`)
		tr = strings.TrimPrefix(tr, `\"`)
		tr = strings.TrimSuffix(tr, `\"`)
		tr = strings.TrimSuffix(tr, `"`)
		tr = strings.Trim(tr, `'`)
		// Only strip trailing period if it follows a quote pattern (like ". or '.)
		// This handles cases like "translation". where the quote was already removed
		if strings.HasSuffix(tr, `"`) || strings.HasSuffix(tr, `'`) {
			tr = strings.TrimRight(tr, ".")
		}
		// Handle remaining embedded quotes like: text"more. -> text
		if idx := strings.Index(tr, `"`); idx >= 0 {
			tr = tr[:idx]
		}
		// Remove any trailing quotes that remain after all above
		for strings.HasSuffix(tr, `"`) || strings.HasSuffix(tr, `'`) {
			tr = strings.TrimSuffix(tr, `"`)
			tr = strings.TrimSuffix(tr, `'`)
		}
		lastTranslation = tr
		lastConfidence = confidenceFromInterface(result["confidence"])
		found = true
	}
	if found {
		return lastTranslation, lastConfidence
	}

	// Strategy 2: outermost { … } — may contain a "translation" key.
	startIdx := bytes.Index(data, []byte("{"))
	endIdx := bytes.LastIndex(data, []byte("}"))
	if startIdx >= 0 && endIdx > startIdx {
		var result map[string]interface{}
		if err := json.Unmarshal(data[startIdx:endIdx+1], &result); err == nil {
			if tr, ok := result["translation"].(string); ok && !isPlaceholder(tr) {
				tr = strings.Trim(tr, `"`)
				tr = strings.TrimPrefix(tr, `\"`)
				tr = strings.TrimSuffix(tr, `\"`)
				tr = strings.TrimSuffix(tr, `"`)
				tr = strings.Trim(tr, `'`)
				if strings.HasSuffix(tr, `"`) || strings.HasSuffix(tr, `'`) {
					tr = strings.TrimRight(tr, ".")
				}
				if idx := strings.Index(tr, `"`); idx >= 0 {
					tr = tr[:idx]
				}
				for strings.HasSuffix(tr, `"`) || strings.HasSuffix(tr, `'`) {
					tr = strings.TrimSuffix(tr, `"`)
					tr = strings.TrimSuffix(tr, `'`)
				}
				return tr, confidenceFromInterface(result["confidence"])
			}
		}
	}

	// Strategy 3: pipe-separated "translation | 0.95" format.
	if tr, conf, ok := extractPipeSeparated(response); ok {
		conf = confidenceFromInterface(conf) // apply cap
		log.Printf("[LLM] Extracted pipe-separated translation: %q (confidence %.2f)", tr, conf)
		return tr, conf
	}

	// Strategy 4: free-text with "Translation:" label.
	if tr := extractFreeTextTranslation(response); tr != "" {
		log.Printf("[LLM] Extracted translation from free-text label: %q", tr)
		return tr, 0.8 // already capped via confidenceFromInterface in parseResponse
	}

	// Strategy 5: plain text — first meaningful line is the translation.
	if tr := extractPlainText(response); tr != "" {
		log.Printf("[LLM] Using plain-text response as translation: %q", tr)
		return tr, 0.8
	}

	log.Printf("[LLM DEBUG] Could not extract translation from response: %q", response)
	return strings.TrimSpace(response), 0.5
}

// extractPipeSeparated parses "translated text | 0.95" from the first
// non-empty line that contains a pipe character followed by a number.
func extractPipeSeparated(response string) (string, float64, bool) {
	for _, line := range strings.Split(response, "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		pipeIdx := strings.LastIndex(line, "|")
		if pipeIdx < 0 {
			continue
		}
		tr := strings.TrimSpace(line[:pipeIdx])
		confStr := strings.TrimSpace(line[pipeIdx+1:])

		// Strip surrounding quotes from the translation part.
		tr = strings.Trim(tr, `"`)
		tr = strings.TrimPrefix(tr, `\"`)
		tr = strings.TrimSuffix(tr, `\"`)
		tr = strings.TrimSuffix(tr, `"`)
		tr = strings.Trim(tr, `'`)
		if strings.HasSuffix(tr, `"`) || strings.HasSuffix(tr, `'`) {
			tr = strings.TrimRight(tr, ".")
		}
		if idx := strings.Index(tr, `"`); idx >= 0 {
			tr = tr[:idx]
		}
		for strings.HasSuffix(tr, `"`) || strings.HasSuffix(tr, `'`) {
			tr = strings.TrimSuffix(tr, `"`)
			tr = strings.TrimSuffix(tr, `'`)
		}

		if tr == "" || isPlaceholder(tr) {
			continue
		}

		// Parse confidence from the string part after the pipe
		var conf float64
		if _, err := fmt.Sscanf(confStr, "%f", &conf); err != nil {
			conf = 0.8 // default when parsing fails
		}
		return tr, conf, true
	}
	return "", 0, false
}

// isPlaceholder returns true if the string looks like a prompt placeholder
// the model copied verbatim instead of translating.
func isPlaceholder(s string) bool {
	s = strings.TrimSpace(strings.ToLower(s))
	return s == "" ||
		s == "<translate>" ||
		s == "hallo welt" ||
		strings.Contains(s, "translated text here") ||
		strings.Contains(s, "your translation here") ||
		strings.Contains(s, "dein text hier") ||
		strings.Contains(s, "corrected text here")
}

// freeTextTranslationRe matches patterns like:
//
//	### Translation:\n"some text"
//	Translation: "some text"
//	Translation: some text
var freeTextTranslationRe = regexp.MustCompile(`(?i)(?:^|\n)#{0,4}\s*Translation\s*:\s*\n?\s*"?([^"\n][^\n]*?)(?:"|$)`)

// extractFreeTextTranslation looks for a "Translation:" label in the model
// output and returns the first non-empty value it finds after that label.
func extractFreeTextTranslation(response string) string {
	matches := freeTextTranslationRe.FindAllStringSubmatch(response, -1)
	if len(matches) == 0 {
		return ""
	}
	tr := strings.TrimSpace(matches[0][1])
	// Strip surrounding quotes (including escaped quotes)
	tr = strings.Trim(tr, `"`)
	tr = strings.TrimPrefix(tr, `\"`)
	tr = strings.TrimSuffix(tr, `\"`)
	tr = strings.TrimSuffix(tr, `"`)
	tr = strings.Trim(tr, `'`)
	if strings.HasSuffix(tr, `"`) || strings.HasSuffix(tr, `'`) {
		tr = strings.TrimRight(tr, ".")
	}
	if idx := strings.Index(tr, `"`); idx >= 0 {
		tr = tr[:idx]
	}
	for strings.HasSuffix(tr, `"`) || strings.HasSuffix(tr, `'`) {
		tr = strings.TrimSuffix(tr, `"`)
		tr = strings.TrimSuffix(tr, `'`)
	}
	if isPlaceholder(tr) {
		return ""
	}
	return tr
}

// skipPrefixes are labels/headers the model sometimes emits before the
// actual translation. We skip these lines when extracting plain text.
var skipPrefixes = []string{
	"translate", "translation", "source", "json", "response",
	"###", "##", "#", "```", "---", "note:", "confidence",
}

// extractPlainText returns the first meaningful line from a free-form model
// response, stripping surrounding quotes and skipping label lines.
func extractPlainText(response string) string {
	for _, line := range strings.Split(response, "\n") {
		line = strings.TrimSpace(line)
		if line == "" {
			continue
		}
		// Strip surrounding quotes (including escaped quotes).
		line = strings.Trim(line, `"`)
		line = strings.TrimPrefix(line, `\"`)
		line = strings.TrimSuffix(line, `\"`)
		line = strings.TrimSuffix(line, `"`)
		line = strings.Trim(line, `'`)
		if strings.HasSuffix(line, `"`) || strings.HasSuffix(line, `'`) {
			line = strings.TrimRight(line, ".")
		}
		if idx := strings.Index(line, `"`); idx >= 0 {
			line = line[:idx]
		}
		for strings.HasSuffix(line, `"`) || strings.HasSuffix(line, `'`) {
			line = strings.TrimSuffix(line, `"`)
			line = strings.TrimSuffix(line, `'`)
		}
		line = strings.TrimSpace(line)
		if line == "" || isPlaceholder(line) {
			continue
		}
		lower := strings.ToLower(line)
		skip := false
		for _, pfx := range skipPrefixes {
			if strings.HasPrefix(lower, pfx) {
				skip = true
				break
			}
		}
		if skip {
			continue
		}
		// Skip lines that look like JSON fragments.
		if strings.HasPrefix(line, "{") || strings.HasPrefix(line, "}") {
			continue
		}
		return line
	}
	return ""
}
