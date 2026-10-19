package routes

import "testing"

// TestSplitLocizeKey covers the locize-style `namespace:key` delimiter used by
// ImportTranslations. The table is deliberately skewed toward edge cases
// (invalid prefixes, multiple colons, empty halves) because a false positive
// here would silently re-bucket a reviewer's keys into the wrong namespace —
// nastier than just failing the import.
func TestSplitLocizeKey(t *testing.T) {
	const defaultNS = "translation"

	cases := []struct {
		name     string
		in       string
		wantNS   string
		wantKey  string
	}{
		// Happy paths — the reason this function exists.
		{"simple locize", "billing:invoiceDueSoon", "billing", "invoiceDueSoon"},
		{"common namespace", "common:buttons.ok", "common", "buttons.ok"},
		{"namespace with dash", "my-ns:greeting", "my-ns", "greeting"},
		{"namespace with dot", "my.ns:greeting", "my.ns", "greeting"},
		{"namespace with underscore", "my_ns:greeting", "my_ns", "greeting"},

		// First-colon-only: anything after the first `:` is the key verbatim.
		{"multiple colons split first", "billing:url:help", "billing", "url:help"},

		// Invalid / non-locize keys fall back to the default namespace.
		{"no colon", "plain.key", defaultNS, "plain.key"},
		{"empty namespace half", ":orphan", defaultNS, ":orphan"},
		{"empty key half", "billing:", defaultNS, "billing:"},
		{"namespace with space", "my ns:greeting", defaultNS, "my ns:greeting"},
		{"namespace with slash", "https://example.com", defaultNS, "https://example.com"},
		{"just a colon", ":", defaultNS, ":"},
		{"empty string", "", defaultNS, ""},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			gotNS, gotKey := splitLocizeKey(tc.in, defaultNS)
			if gotNS != tc.wantNS || gotKey != tc.wantKey {
				t.Errorf("splitLocizeKey(%q) = (%q, %q), want (%q, %q)",
					tc.in, gotNS, gotKey, tc.wantNS, tc.wantKey)
			}
		})
	}
}
