package llm

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"sync"
	"testing"
)

func TestCheckGlossaryViolations(t *testing.T) {
	cases := []struct {
		name        string
		translation string
		glossary    []GlossaryTerm
		wantCount   int
	}{
		{
			name:        "exact match passes",
			translation: "Har du glömt din e-post?",
			glossary:    []GlossaryTerm{{SourceTerm: "email", TargetTerm: "e-post"}},
			wantCount:   0,
		},
		{
			name:        "exact match at start",
			translation: "E-post address is required.",
			glossary:    []GlossaryTerm{{SourceTerm: "email", TargetTerm: "e-post"}},
			wantCount:   0,
		},
		{
			name:        "compound form is a violation",
			translation: "Den här e-postadressen är redan registrerad.",
			glossary:    []GlossaryTerm{{SourceTerm: "email", TargetTerm: "e-post"}},
			wantCount:   1,
		},
		{
			name:        "compound with umlaut is a violation",
			translation: "e-postmeddelande skickat.",
			glossary:    []GlossaryTerm{{SourceTerm: "email", TargetTerm: "e-post"}},
			wantCount:   1,
		},
		{
			name:        "case insensitive match",
			translation: "E-POST är bra",
			glossary:    []GlossaryTerm{{SourceTerm: "email", TargetTerm: "e-post"}},
			wantCount:   0,
		},
		{
			name:        "punctuation is a boundary",
			translation: "Skicka e-post.",
			glossary:    []GlossaryTerm{{SourceTerm: "email", TargetTerm: "e-post"}},
			wantCount:   0,
		},
		{
			name:        "trailing non-ASCII letter is a violation",
			translation: "e-postå",
			glossary:    []GlossaryTerm{{SourceTerm: "email", TargetTerm: "e-post"}},
			wantCount:   1,
		},
		{
			name:        "missing entirely is a violation",
			translation: "Skicka meddelande.",
			glossary:    []GlossaryTerm{{SourceTerm: "email", TargetTerm: "e-post"}},
			wantCount:   1,
		},
		{
			name:        "multiple rules, one violated",
			translation: "Logga in med e-post och lösen.",
			glossary: []GlossaryTerm{
				{SourceTerm: "email", TargetTerm: "e-post"},
				{SourceTerm: "password", TargetTerm: "lösenord"},
			},
			wantCount: 1,
		},
		{
			name:        "empty glossary returns nil",
			translation: "anything",
			glossary:    nil,
			wantCount:   0,
		},
		{
			// The whole reason variants exist: "e-posten" is the grammatical
			// definite form of "e-post" in Swedish and MUST be accepted when
			// the reviewer whitelists it.
			name:        "whitelisted definite variant passes",
			translation: "E-posten är redan registrerad.",
			glossary: []GlossaryTerm{{
				SourceTerm:     "email",
				TargetTerm:     "e-post",
				TargetVariants: []string{"e-posten", "e-poster", "e-posterna"},
			}},
			wantCount: 0,
		},
		{
			// Variants don't rescue genuine compounds — "e-postadressen"
			// has "adressen" glued on and must still fail boundary check
			// regardless of which variants are accepted.
			name:        "compound still violates even with variants allowed",
			translation: "E-postadressen eller lösenordet är felaktigt.",
			glossary: []GlossaryTerm{{
				SourceTerm:     "email",
				TargetTerm:     "e-post",
				TargetVariants: []string{"e-posten", "e-poster", "e-posterna"},
			}},
			wantCount: 1,
		},
		{
			// Only the variant is present, canonical is missing → still ok
			// because the validator accepts ANY listed form.
			name:        "variant alone is sufficient",
			translation: "Kolla e-posterna nu.",
			glossary: []GlossaryTerm{{
				SourceTerm:     "email",
				TargetTerm:     "e-post",
				TargetVariants: []string{"e-posten", "e-poster", "e-posterna"},
			}},
			wantCount: 0,
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := checkGlossaryViolations(tc.translation, tc.glossary)
			if len(got) != tc.wantCount {
				t.Errorf("checkGlossaryViolations: want %d violations, got %d (%v)",
					tc.wantCount, len(got), got)
			}
		})
	}
}

// fakeLLMServer stands in for llama-server's /completion endpoint. The
// `responses` slice is consumed in order — one body per request — so a test
// can script exactly what the model "returns" on the first attempt vs the
// retries. Once the script runs out, further calls return the last entry.
func fakeLLMServer(t *testing.T, responses []string) (*httptest.Server, *int) {
	t.Helper()
	var (
		mu    sync.Mutex
		calls int
	)
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		mu.Lock()
		idx := calls
		calls++
		mu.Unlock()

		body := responses[len(responses)-1]
		if idx < len(responses) {
			body = responses[idx]
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(map[string]string{"content": body})
	}))
	t.Cleanup(srv.Close)
	return srv, &calls
}

func TestTranslateWithGlossary_AutoRetriesOnViolation(t *testing.T) {
	// First response violates the rule ("e-postadressen"), second fixes it.
	srv, calls := fakeLLMServer(t, []string{
		`{"translation":"E-postadressen är redan registrerad.","confidence":0.9}`,
		`{"translation":"E-post är redan registrerad.","confidence":0.9}`,
	})
	client := NewClient(srv.URL, "test-model", false)

	resp, err := client.TranslateWithGlossary("en", "sv",
		"Email is already registered.", 0.2,
		[]GlossaryTerm{{SourceTerm: "email", TargetTerm: "e-post"}},
		"",
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if *calls != 2 {
		t.Errorf("want 2 LLM calls (one retry), got %d", *calls)
	}
	if len(resp.GlossaryViolations) != 0 {
		t.Errorf("want no violations after successful retry, got %v", resp.GlossaryViolations)
	}
	if resp.Text != "E-post är redan registrerad." {
		t.Errorf("want retried text, got %q", resp.Text)
	}
	if resp.Confidence != 0.9 {
		t.Errorf("want confidence preserved from successful retry, got %v", resp.Confidence)
	}
}

func TestTranslateWithGlossary_GivesUpAfterMaxRetries(t *testing.T) {
	// Every response violates — we should see 1 initial + maxGlossaryRetries
	// additional calls, then return with violations populated and confidence
	// capped at 0.5.
	srv, calls := fakeLLMServer(t, []string{
		`{"translation":"E-postadressen är redan registrerad.","confidence":0.9}`,
	})
	client := NewClient(srv.URL, "test-model", false)

	resp, err := client.TranslateWithGlossary("en", "sv",
		"Email is already registered.", 0.2,
		[]GlossaryTerm{{SourceTerm: "email", TargetTerm: "e-post"}},
		"",
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	wantCalls := 1 + maxGlossaryRetries
	if *calls != wantCalls {
		t.Errorf("want %d LLM calls, got %d", wantCalls, *calls)
	}
	if len(resp.GlossaryViolations) != 1 {
		t.Errorf("want 1 violation surfaced, got %v", resp.GlossaryViolations)
	}
	if resp.Confidence > 0.5 {
		t.Errorf("want confidence capped at 0.5 after exhausted retries, got %v", resp.Confidence)
	}
}

func TestTranslateWithGlossary_NoRetryWhenFirstAttemptClean(t *testing.T) {
	srv, calls := fakeLLMServer(t, []string{
		`{"translation":"E-post är redan registrerad.","confidence":0.9}`,
	})
	client := NewClient(srv.URL, "test-model", false)

	resp, err := client.TranslateWithGlossary("en", "sv",
		"Email is already registered.", 0.2,
		[]GlossaryTerm{{SourceTerm: "email", TargetTerm: "e-post"}},
		"",
	)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if *calls != 1 {
		t.Errorf("clean first attempt should not retry; got %d calls", *calls)
	}
	if len(resp.GlossaryViolations) != 0 {
		t.Errorf("want no violations, got %v", resp.GlossaryViolations)
	}
	if resp.Confidence != 0.9 {
		t.Errorf("want confidence preserved, got %v", resp.Confidence)
	}
}
