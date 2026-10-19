package translation

import (
	"path/filepath"
	"testing"

	"glyph/internal/db"
)

// A key only the target has is not a translated source key: it stays out of
// translated and total, and translated + missing still adds up to total.
func TestLanguagePairStatsIgnoresTargetOnlyKeys(t *testing.T) {
	dir := t.TempDir()
	store, err := db.Open(filepath.Join(dir, "glyph.db"))
	if err != nil {
		t.Fatalf("open db: %v", err)
	}
	defer store.Close()
	m, err := NewManager(filepath.Join(dir, "translations"), store)
	if err != nil {
		t.Fatalf("new manager: %v", err)
	}

	set := func(lang, key, value string) {
		t.Helper()
		if err := m.SetTranslation(lang, "lair", key, value); err != nil {
			t.Fatalf("set %s %s: %v", lang, key, err)
		}
	}
	set("en", "door", "Door")
	set("en", "moat", "Moat")
	set("en", "trap", "Trap")
	set("sv", "door", "Dörr")
	set("sv", "moat", "Vallgrav")
	set("sv", "secretTunnel", "Hemlig tunnel") // not in English

	s := m.GetLanguagePairStats("en", "sv", "lair")
	if got := s["total_keys"]; got != 3 {
		t.Errorf("total_keys = %v, want 3", got)
	}
	if got := s["translated_keys"]; got != 2 {
		t.Errorf("translated_keys = %v, want 2", got)
	}
	if got := s["missing_count"]; got != 1 {
		t.Errorf("missing_count = %v, want 1", got)
	}

	back := m.GetMissingSourceKeys("en", "lair")
	if len(back) != 1 || back[0].Key != "secretTunnel" {
		t.Errorf("missing source keys = %+v, want only secretTunnel", back)
	}
}
