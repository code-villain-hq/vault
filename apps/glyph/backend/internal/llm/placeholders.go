package llm

import (
	"fmt"
	"regexp"
	"sort"
	"strings"
)

// mustachePlaceholderRegexp matches i18n template tokens like {{count}},
// {{ name }}, {{ user.email }} — non-greedy so a line with two tokens yields
// two separate matches.
var mustachePlaceholderRegexp = regexp.MustCompile(`\{\{[\s\S]*?\}\}`)

// mustachePreserveInstructions is inlined into every translation prompt so the
// model never translates or rewrites text inside {{ }} regardless of target
// language.
const mustachePreserveInstructions = `
Template placeholders — CRITICAL (apply only to tokens that appear in the SOURCE string below, not to examples elsewhere in this prompt):
  * If the source contains Mustache-style tokens (segments starting with "{{" and ending with the next "}}"), copy each one into your translation EXACTLY as in the source: same characters and spacing inside the braces. Do NOT translate or rename anything between {{ and }}.
  * You may move a whole token elsewhere in the sentence for grammar, but each token string must remain byte-identical to how it appears in the source.
  * If the source has NO such tokens, your translation must have NONE — do not invent "{{...}}" from instructions, JSON examples, or glossaries.
  * Do not drop or merge tokens that exist in the source.`

func mustachePlaceholderCounts(s string) map[string]int {
	m := make(map[string]int)
	for _, p := range mustachePlaceholderRegexp.FindAllString(s, -1) {
		m[p]++
	}
	return m
}

// mustachePlaceholdersPreserved reports whether Mustache placeholder rules hold:
// if the source has no "{{...}}" tokens, the translation must not contain any;
// otherwise every token from the source must appear in the translation at least
// as often as in the source (multiset containment). Extra distinct tokens in the
// translation are allowed only when the source already contained placeholders.
func mustachePlaceholdersPreserved(source, translation string) bool {
	src := mustachePlaceholderCounts(source)
	tgt := mustachePlaceholderCounts(translation)
	if len(src) == 0 {
		return len(tgt) == 0
	}
	for tok, n := range src {
		if tgt[tok] < n {
			return false
		}
	}
	return true
}

// completionJSONSuffix is appended to correction prompts in completion mode
// (no system message) so the model knows to reply with JSON.
const completionJSONSuffix = "\n\nReply ONLY with a JSON object with two fields: \"translation\" (the corrected text) and \"confidence\" (a number between 0 and 1).\n\nJSON:"

func buildPlaceholderCorrectionPrompt(sourceLang, targetLang, sourceText, badTranslation, usageContext string, chatMode bool) string {
	src := mustachePlaceholderCounts(sourceText)
	tgt := mustachePlaceholderCounts(badTranslation)

	ctxNote := ""
	if t := strings.TrimSpace(usageContext); t != "" {
		ctxNote = fmt.Sprintf("\nProduct / UI context (keep in mind while fixing placeholders):\n%s\n", t)
	}

	formatSuffix := "\n\nProvide the corrected translation."
	if !chatMode {
		formatSuffix = completionJSONSuffix
	}

	// Source is plain language but the model invented "{{...}}" tokens.
	if len(src) == 0 && len(tgt) > 0 {
		var invented []string
		for tok := range tgt {
			invented = append(invented, tok)
		}
		sort.Strings(invented)
		return fmt.Sprintf(`You are a professional translator. Your previous translation from %s to %s is INVALID.

The source text contains NO Mustache-style "{{...}}" template tokens — only plain words and punctuation.
Your translation incorrectly invented template tokens (remove ALL of them; the output must be natural language only): %s

Source text: %q
Your previous (INCORRECT) translation: %q
%s
Rewrite the translation into the target language with ZERO "{{" or "}}" template syntax.%s`, sourceLang, targetLang, strings.Join(invented, ", "), sourceText, badTranslation, ctxNote, formatSuffix)
	}

	var problems []string
	for tok, want := range src {
		have := tgt[tok]
		if have < want {
			problems = append(problems, fmt.Sprintf("%q must appear exactly %d time(s) in the translation (found %d).", tok, want, have))
		}
	}
	problemBlock := strings.Join(problems, "\n  * ")
	if problemBlock != "" {
		problemBlock = "  * " + problemBlock
	}

	return fmt.Sprintf(`You are a professional translator. Your previous translation from %s to %s broke CRITICAL rules about template placeholders.

Source text: %q
Your previous (INCORRECT) translation: %q
%s
The following placeholder tokens from the source are missing or were altered (each {{ ... }} block must appear verbatim, same number of times as in the source):
%s

Rewrite the translation so EVERY "{{ ... }}" token from the source appears in your output EXACTLY unchanged — same text inside the braces, same braces. Translate only the natural-language parts outside these tokens.%s`, sourceLang, targetLang, sourceText, badTranslation, ctxNote, problemBlock, formatSuffix)
}
