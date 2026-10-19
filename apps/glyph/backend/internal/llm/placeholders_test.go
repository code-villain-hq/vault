package llm

import "testing"

func TestMustachePlaceholdersPreserved(t *testing.T) {
	t.Parallel()
	if !mustachePlaceholdersPreserved(`Hi {{name}}`, `Hej {{name}}`) {
		t.Fatal("expected preserved when token identical")
	}
	if mustachePlaceholdersPreserved(`{{a}} {{a}}`, `{{a}}`) {
		t.Fatal("expected false when multiset count differs")
	}
	if mustachePlaceholdersPreserved(`Hello {{name}} there`, `Hello there`) {
		t.Fatal("expected false when placeholder dropped")
	}
	if !mustachePlaceholdersPreserved(`no placeholders`, `inget`) {
		t.Fatal("expected pass when source and translation have no template tokens")
	}
	if mustachePlaceholdersPreserved(`no placeholders`, `{{count}} {{name}}s`) {
		t.Fatal("expected false when translation invents placeholders")
	}
}

func TestTranslateWithGlossary_RetriesWhenPlaceholderDropped(t *testing.T) {
	t.Parallel()
	// No glossary — only placeholder enforcement. First drops {{name}}, second fixes.
	srv, calls := fakeLLMServer(t, []string{
		`{"translation":"Hej name!","confidence":0.9}`,
		`{"translation":"Hej {{name}}!","confidence":0.85}`,
	})
	client := NewClient(srv.URL, "test-model", false)

	resp, err := client.TranslateWithGlossary("en", "sv",
		`Hello {{name}}!`, 0.2, nil, "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if *calls != 2 {
		t.Errorf("want 2 LLM calls (placeholder retry), got %d", *calls)
	}
	if resp.Text != "Hej {{name}}!" {
		t.Errorf("want retried text with token, got %q", resp.Text)
	}
}

func TestTranslateWithGlossary_RetriesWhenPlaceholderInvented(t *testing.T) {
	t.Parallel()
	srv, calls := fakeLLMServer(t, []string{
		`{"translation":"{{count}} {{name}}s {{user.email}}s","confidence":0.9}`,
		`{"translation":"Ankunftsortname fehlt.","confidence":0.85}`,
	})
	client := NewClient(srv.URL, "test-model", false)

	resp, err := client.TranslateWithGlossary("en", "de",
		`Arrival location name missing.`, 0.2, nil, "")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if *calls != 2 {
		t.Errorf("want 2 LLM calls (placeholder retry), got %d", *calls)
	}
	if resp.Text != "Ankunftsortname fehlt." {
		t.Errorf("want retried plain translation, got %q", resp.Text)
	}
}
