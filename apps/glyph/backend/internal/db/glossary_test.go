package db

import (
	"path/filepath"
	"strings"
	"testing"
)

func TestGlossaryMigrationAndCRUD(t *testing.T) {
	path := filepath.Join(t.TempDir(), "glossary.db")
	d, err := Open(path)
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer d.Close()

	id, err := d.AddGlossaryEntry(GlossaryEntry{SourceLang: "en", TargetLang: "sv", SourceTerm: "Email", TargetTerm: "e-post", Note: "product voice"})
	if err != nil {
		t.Fatalf("add: %v", err)
	}
	if id <= 0 {
		t.Fatalf("expected positive id, got %d", id)
	}

	// Duplicate insert (case-insensitive) must fail.
	if _, err := d.AddGlossaryEntry(GlossaryEntry{SourceLang: "en", TargetLang: "sv", SourceTerm: "EMAIL", TargetTerm: "x"}); err == nil {
		t.Fatal("expected UNIQUE constraint error on duplicate")
	} else if !strings.Contains(err.Error(), "UNIQUE") {
		t.Fatalf("expected UNIQUE error, got %v", err)
	}

	// Same source term but different pair should be fine.
	if _, err := d.AddGlossaryEntry(GlossaryEntry{SourceLang: "en", TargetLang: "fi", SourceTerm: "Email", TargetTerm: "sähköposti"}); err != nil {
		t.Fatalf("cross-pair same term should succeed: %v", err)
	}

	if err := d.UpdateGlossaryEntry(id, GlossaryEntry{SourceTerm: "email", TargetTerm: "e-postadress", Note: "v2"}); err != nil {
		t.Fatalf("update: %v", err)
	}

	entries, err := d.ListGlossary("en", "sv")
	if err != nil {
		t.Fatalf("list: %v", err)
	}
	if len(entries) != 1 {
		t.Fatalf("expected 1 entry for en→sv, got %d", len(entries))
	}
	if entries[0].TargetTerm != "e-postadress" || entries[0].Note != "v2" {
		t.Fatalf("unexpected entry after update: %+v", entries[0])
	}

	all, err := d.ListGlossary("", "")
	if err != nil {
		t.Fatalf("list all: %v", err)
	}
	if len(all) != 2 {
		t.Fatalf("expected 2 total entries, got %d", len(all))
	}

	if err := d.DeleteGlossaryEntry(id); err != nil {
		t.Fatalf("delete: %v", err)
	}
	rem, _ := d.ListGlossary("en", "sv")
	if len(rem) != 0 {
		t.Fatalf("expected 0 en→sv after delete, got %d", len(rem))
	}
}

func TestGlossaryTargetVariantsRoundTrip(t *testing.T) {
	path := filepath.Join(t.TempDir(), "variants.db")
	d, err := Open(path)
	if err != nil {
		t.Fatalf("open: %v", err)
	}
	defer d.Close()

	id, err := d.AddGlossaryEntry(GlossaryEntry{
		SourceLang:     "en",
		TargetLang:     "sv",
		SourceTerm:     "email",
		TargetTerm:     "e-post",
		TargetVariants: []string{"e-posten", "e-poster", "e-posterna", "  ", ""},
		Note:           "swedish inflections",
	})
	if err != nil {
		t.Fatalf("add: %v", err)
	}

	entries, err := d.ListGlossary("en", "sv")
	if err != nil || len(entries) != 1 {
		t.Fatalf("list: err=%v, entries=%+v", err, entries)
	}
	got := entries[0].TargetVariants
	want := []string{"e-posten", "e-poster", "e-posterna"}
	if len(got) != len(want) {
		t.Fatalf("variants: got %v, want %v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("variants[%d]: got %q, want %q", i, got[i], want[i])
		}
	}

	// Updating to an empty list should clear the column.
	if err := d.UpdateGlossaryEntry(id, GlossaryEntry{
		SourceTerm: "email", TargetTerm: "e-post", TargetVariants: nil, Note: "cleared",
	}); err != nil {
		t.Fatalf("update: %v", err)
	}
	entries, _ = d.ListGlossary("en", "sv")
	if len(entries[0].TargetVariants) != 0 {
		t.Fatalf("expected variants cleared, got %+v", entries[0].TargetVariants)
	}
}
