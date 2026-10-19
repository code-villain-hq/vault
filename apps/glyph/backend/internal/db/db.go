// Package db is the persistent storage layer for Glyph.
//
// It uses SQLite (via the pure-Go modernc.org/sqlite driver, so no CGO is
// required for this package) and holds four logical things:
//
//  1. schema_version — a tiny one-row table used to apply in-order migrations.
//  2. translations   — authoritative (lang, namespace, key) -> value store.
//  3. metadata       — confidence / translated_by / timestamp / optional
//                      context per (lang, namespace, key).
//  4. config_kv      — a small key/value bucket used to overlay editable
//                      user_preferences / language_pairs on top of config.yaml.
//
// Namespaces follow the locize / i18next convention: each language has one or
// more namespaces (think "common", "auth", "dashboard"), and keys live inside
// a namespace. Older installs that predate namespaces automatically have all
// of their rows re-keyed to the "translation" namespace when the v2 migration
// runs.
package db

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"strings"

	_ "modernc.org/sqlite"
)

// DefaultNamespace is the namespace used when none is specified, matching the
// locize/i18next default. All pre-namespace rows are migrated into this
// namespace on first run of v2.
const DefaultNamespace = "translation"

// DB wraps *sql.DB with Glyph-specific queries.
type DB struct {
	conn *sql.DB
}

// Open opens (and lazily creates) the SQLite database at path and runs any
// pending migrations. The returned *DB is safe for concurrent use.
func Open(path string) (*DB, error) {
	// _pragma=foreign_keys(1) keeps FKs honest.
	// _pragma=journal_mode(WAL) makes writes faster and concurrent reads cheaper.
	// _pragma=busy_timeout(5000) avoids "database is locked" flakes.
	dsn := fmt.Sprintf(
		"file:%s?_pragma=foreign_keys(1)&_pragma=journal_mode(WAL)&_pragma=busy_timeout(5000)",
		path,
	)

	conn, err := sql.Open("sqlite", dsn)
	if err != nil {
		return nil, fmt.Errorf("open sqlite: %w", err)
	}
	if err := conn.Ping(); err != nil {
		return nil, fmt.Errorf("ping sqlite: %w", err)
	}

	d := &DB{conn: conn}
	if err := d.migrate(); err != nil {
		return nil, fmt.Errorf("migrate: %w", err)
	}
	return d, nil
}

// Close closes the underlying connection pool.
func (d *DB) Close() error {
	return d.conn.Close()
}

// migrate applies migrations in order. Each step bumps schema_version.version
// and its body runs in a single transaction, so a half-applied migration
// rolls back cleanly on error.
func (d *DB) migrate() error {
	if _, err := d.conn.Exec(`
		CREATE TABLE IF NOT EXISTS schema_version (
			version INTEGER PRIMARY KEY
		);
	`); err != nil {
		return err
	}

	var current int
	if err := d.conn.QueryRow(`SELECT COALESCE(MAX(version), 0) FROM schema_version`).Scan(&current); err != nil {
		return err
	}

	// Migrations are strictly additive; add new ones to the end.
	steps := []struct {
		version int
		name    string
		apply   func(tx *sql.Tx) error
	}{
		{1, "initial schema", migrateV1},
		{2, "add namespace column", migrateV2Namespace},
		{3, "add glossary table", migrateV3Glossary},
		{4, "add glossary target_variants column", migrateV4GlossaryVariants},
		{5, "metadata context for LLM", migrateV5MetadataContext},
	}

	for _, s := range steps {
		if s.version <= current {
			continue
		}
		tx, err := d.conn.Begin()
		if err != nil {
			return err
		}
		if err := s.apply(tx); err != nil {
			_ = tx.Rollback()
			return fmt.Errorf("migration %d (%s): %w", s.version, s.name, err)
		}
		if _, err := tx.Exec(`INSERT INTO schema_version (version) VALUES (?)`, s.version); err != nil {
			_ = tx.Rollback()
			return fmt.Errorf("record migration %d: %w", s.version, err)
		}
		if err := tx.Commit(); err != nil {
			return fmt.Errorf("commit migration %d: %w", s.version, err)
		}
	}
	return nil
}

// migrateV1 creates the initial set of tables (pre-namespace layout). When a
// fresh DB runs through migration it just creates the tables as they used to
// be, and v2 immediately rewrites them to include the namespace column. This
// is slightly more work than "create the latest schema directly" but keeps
// migrations explicit and testable.
func migrateV1(tx *sql.Tx) error {
	_, err := tx.Exec(`
		CREATE TABLE IF NOT EXISTS translations (
			lang       TEXT NOT NULL,
			key        TEXT NOT NULL,
			value      TEXT NOT NULL,
			updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
			PRIMARY KEY (lang, key)
		);
		CREATE INDEX IF NOT EXISTS idx_translations_lang ON translations(lang);

		CREATE TABLE IF NOT EXISTS metadata (
			lang          TEXT NOT NULL,
			key           TEXT NOT NULL,
			confidence    REAL NOT NULL DEFAULT 0,
			translated_by TEXT NOT NULL DEFAULT '',
			timestamp     TEXT NOT NULL DEFAULT '',
			PRIMARY KEY (lang, key)
		);

		CREATE TABLE IF NOT EXISTS config_kv (
			key        TEXT PRIMARY KEY,
			value_json TEXT NOT NULL,
			updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
		);
	`)
	return err
}

// migrateV2Namespace rewrites translations and metadata to include a namespace
// column as part of the primary key. Existing rows are re-keyed to
// DefaultNamespace ("translation"), matching locize's conventions.
func migrateV2Namespace(tx *sql.Tx) error {
	// Rebuild translations with namespace column. SQLite doesn't support
	// changing a PK in place, so we create a new table, copy rows with the
	// default namespace, drop the old table, then rename.
	_, err := tx.Exec(`
		CREATE TABLE translations_v2 (
			lang       TEXT NOT NULL,
			namespace  TEXT NOT NULL DEFAULT 'translation',
			key        TEXT NOT NULL,
			value      TEXT NOT NULL,
			updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
			PRIMARY KEY (lang, namespace, key)
		);

		INSERT INTO translations_v2 (lang, namespace, key, value, updated_at)
		SELECT lang, 'translation', key, value, updated_at FROM translations;

		DROP TABLE translations;
		ALTER TABLE translations_v2 RENAME TO translations;

		CREATE INDEX IF NOT EXISTS idx_translations_lang      ON translations(lang);
		CREATE INDEX IF NOT EXISTS idx_translations_lang_ns   ON translations(lang, namespace);

		CREATE TABLE metadata_v2 (
			lang          TEXT NOT NULL,
			namespace     TEXT NOT NULL DEFAULT 'translation',
			key           TEXT NOT NULL,
			confidence    REAL NOT NULL DEFAULT 0,
			translated_by TEXT NOT NULL DEFAULT '',
			timestamp     TEXT NOT NULL DEFAULT '',
			PRIMARY KEY (lang, namespace, key)
		);

		INSERT INTO metadata_v2 (lang, namespace, key, confidence, translated_by, timestamp)
		SELECT lang, 'translation', key, confidence, translated_by, timestamp FROM metadata;

		DROP TABLE metadata;
		ALTER TABLE metadata_v2 RENAME TO metadata;
	`)
	return err
}

// migrateV3Glossary adds the glossary / term-base table. Each row pins a
// source phrase to its preferred target in one language direction — e.g.
// (en, sv, "email") → "e-post". The translation service injects matching
// entries into the LLM prompt so models stop drifting to synonyms.
//
// source_term_lower is kept as a stored column so we can enforce
// case-insensitive uniqueness per (source, target) pair (SQLite doesn't
// support expression-indexed UNIQUE constraints without generated columns).
func migrateV3Glossary(tx *sql.Tx) error {
	_, err := tx.Exec(`
		CREATE TABLE glossary (
			id                INTEGER PRIMARY KEY AUTOINCREMENT,
			source_lang       TEXT NOT NULL,
			target_lang       TEXT NOT NULL,
			source_term       TEXT NOT NULL,
			source_term_lower TEXT NOT NULL,
			target_term       TEXT NOT NULL,
			note              TEXT NOT NULL DEFAULT '',
			created_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
			updated_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
			UNIQUE (source_lang, target_lang, source_term_lower)
		);
		CREATE INDEX idx_glossary_pair ON glossary(source_lang, target_lang);
	`)
	return err
}

// migrateV4GlossaryVariants adds a `target_variants` column that stores
// additional accepted forms of the canonical target_term — e.g. for Swedish
// `e-post` the reviewer can also whitelist `e-posten` (definite), `e-poster`
// (indef. plural) and `e-posterna` (def. plural) so the validator doesn't
// flag grammatically required inflections as violations.
//
// Stored as a newline-separated string (newline is safe because variants are
// always single words). NULL-safe via DEFAULT ''; empty = strict matching,
// identical to pre-v4 behaviour.
// migrateV5MetadataContext adds optional reviewer context per metadata row so
// the LLM can be told where a string appears in the product.
func migrateV5MetadataContext(tx *sql.Tx) error {
	_, err := tx.Exec(`ALTER TABLE metadata ADD COLUMN context TEXT NOT NULL DEFAULT '';`)
	return err
}

func migrateV4GlossaryVariants(tx *sql.Tx) error {
	_, err := tx.Exec(`ALTER TABLE glossary ADD COLUMN target_variants TEXT NOT NULL DEFAULT ''`)
	return err
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

// GlossaryEntry is a single term-base rule: "in sv, always translate email as
// e-post". source_lang / target_lang are BCP-47-ish codes matching the rest of
// the app. note is free-form context shown to reviewers (e.g. "consumer
// product voice") and is never sent to the LLM.
//
// TargetVariants is an optional list of additional accepted forms of
// TargetTerm — used by the validator to allow grammatical inflections (e.g.
// Swedish definite "e-posten" for "e-post") without flagging them as glossary
// violations. Empty ⇒ strict exact-word matching on TargetTerm only.
type GlossaryEntry struct {
	ID             int64
	SourceLang     string
	TargetLang     string
	SourceTerm     string
	TargetTerm     string
	TargetVariants []string
	Note           string
	CreatedAt      string
	UpdatedAt      string
}

// Translation is a single namespaced translation row.
type Translation struct {
	Lang      string
	Namespace string
	Key       string
	Value     string
}

// KeyMetadata mirrors translation.KeyMetadata but lives here to avoid an
// import cycle.
type KeyMetadata struct {
	Confidence   float64
	TranslatedBy string
	Timestamp    string
	Context      string
}

// ---------------------------------------------------------------------------
// Translations
// ---------------------------------------------------------------------------

// CountTranslations returns the total number of translation rows, across all
// languages and namespaces. Used to decide whether a fresh DB should be
// seeded from JSON.
func (d *DB) CountTranslations() (int, error) {
	var n int
	err := d.conn.QueryRow(`SELECT COUNT(*) FROM translations`).Scan(&n)
	return n, err
}

// ListLanguages returns every distinct language code that has at least one
// translation row (in any namespace).
func (d *DB) ListLanguages() ([]string, error) {
	rows, err := d.conn.Query(`SELECT DISTINCT lang FROM translations ORDER BY lang`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []string
	for rows.Next() {
		var lang string
		if err := rows.Scan(&lang); err != nil {
			return nil, err
		}
		out = append(out, lang)
	}
	return out, rows.Err()
}

// ListNamespaces returns every distinct namespace code across the DB, or
// scoped to a single language if lang != "".
func (d *DB) ListNamespaces(lang string) ([]string, error) {
	var (
		rows *sql.Rows
		err  error
	)
	if lang == "" {
		rows, err = d.conn.Query(`SELECT DISTINCT namespace FROM translations ORDER BY namespace`)
	} else {
		rows, err = d.conn.Query(`SELECT DISTINCT namespace FROM translations WHERE lang = ? ORDER BY namespace`, lang)
	}
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []string
	for rows.Next() {
		var ns string
		if err := rows.Scan(&ns); err != nil {
			return nil, err
		}
		out = append(out, ns)
	}
	return out, rows.Err()
}

// GetTranslations returns every key→value pair for (lang, namespace).
func (d *DB) GetTranslations(lang, namespace string) (map[string]string, error) {
	rows, err := d.conn.Query(
		`SELECT key, value FROM translations WHERE lang = ? AND namespace = ?`,
		lang, namespace,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make(map[string]string)
	for rows.Next() {
		var k, v string
		if err := rows.Scan(&k, &v); err != nil {
			return nil, err
		}
		out[k] = v
	}
	return out, rows.Err()
}

// GetAllTranslationsForLang returns every row for a language grouped by
// namespace -> key -> value. Convenience for bulk reads.
func (d *DB) GetAllTranslationsForLang(lang string) (map[string]map[string]string, error) {
	rows, err := d.conn.Query(
		`SELECT namespace, key, value FROM translations WHERE lang = ?`,
		lang,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make(map[string]map[string]string)
	for rows.Next() {
		var ns, k, v string
		if err := rows.Scan(&ns, &k, &v); err != nil {
			return nil, err
		}
		if _, ok := out[ns]; !ok {
			out[ns] = make(map[string]string)
		}
		out[ns][k] = v
	}
	return out, rows.Err()
}

// SetTranslation inserts-or-updates a single (lang, namespace, key) -> value.
func (d *DB) SetTranslation(lang, namespace, key, value string) error {
	_, err := d.conn.Exec(`
		INSERT INTO translations (lang, namespace, key, value)
		VALUES (?, ?, ?, ?)
		ON CONFLICT(lang, namespace, key) DO UPDATE SET
			value = excluded.value,
			updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
	`, lang, namespace, key, value)
	return err
}

// DeleteLanguage removes every translation and every metadata row for a
// language. Used when the user retires a target language entirely (so the
// matrix, dashboard and review pages all stop showing it).
func (d *DB) DeleteLanguage(lang string) error {
	tx, err := d.conn.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback() //nolint:errcheck

	if _, err := tx.Exec(`DELETE FROM translations WHERE lang = ?`, lang); err != nil {
		return err
	}
	if _, err := tx.Exec(`DELETE FROM metadata WHERE lang = ?`, lang); err != nil {
		return err
	}
	return tx.Commit()
}

// DeleteKeyEverywhere removes (namespace, key) from every language at once,
// dropping both the translation rows and the metadata rows in a single
// transaction. Returns the set of (lang, namespace) pairs that had a row for
// this key so the caller can re-export the affected JSON files.
func (d *DB) DeleteKeyEverywhere(namespace, key string) (affected []LangNamespace, err error) {
	tx, err := d.conn.Begin()
	if err != nil {
		return nil, err
	}
	defer tx.Rollback() //nolint:errcheck

	// Collect everyone who had this (ns, key) before we wipe it.
	rows, err := tx.Query(
		`SELECT DISTINCT lang FROM translations WHERE namespace = ? AND key = ?`,
		namespace, key,
	)
	if err != nil {
		return nil, err
	}
	for rows.Next() {
		var lang string
		if err := rows.Scan(&lang); err != nil {
			rows.Close()
			return nil, err
		}
		affected = append(affected, LangNamespace{Lang: lang, Namespace: namespace})
	}
	rows.Close()

	if _, err := tx.Exec(
		`DELETE FROM translations WHERE namespace = ? AND key = ?`,
		namespace, key,
	); err != nil {
		return nil, err
	}
	if _, err := tx.Exec(
		`DELETE FROM metadata WHERE namespace = ? AND key = ?`,
		namespace, key,
	); err != nil {
		return nil, err
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return affected, nil
}

// LangNamespace is a (lang, namespace) tuple returned by DeleteKeyEverywhere.
type LangNamespace struct {
	Lang      string
	Namespace string
}

// RenameNamespace re-buckets every (translations + metadata) row from `old`
// into `new`, across every language, in one transaction. Returns the list of
// languages that actually had rows under `old` so the caller can re-export
// the affected JSON files.
//
// Fails if `new` already has rows (to avoid accidentally merging two distinct
// buckets and silently overwriting keys). Callers should surface that as a
// user-visible validation error.
func (d *DB) RenameNamespace(oldNS, newNS string) (affectedLangs []string, err error) {
	if oldNS == "" || newNS == "" {
		return nil, fmt.Errorf("both old and new namespace are required")
	}
	if oldNS == newNS {
		return nil, nil
	}

	tx, err := d.conn.Begin()
	if err != nil {
		return nil, err
	}
	defer tx.Rollback() //nolint:errcheck

	// Guard: if `new` already has rows, refuse. Merging buckets is a
	// destructive operation we don't want to do implicitly.
	var existing int
	if err := tx.QueryRow(
		`SELECT COUNT(*) FROM translations WHERE namespace = ?`, newNS,
	).Scan(&existing); err != nil {
		return nil, err
	}
	if existing > 0 {
		return nil, fmt.Errorf("namespace %q already has %d rows; pick a different name", newNS, existing)
	}

	rows, err := tx.Query(
		`SELECT DISTINCT lang FROM translations WHERE namespace = ?`, oldNS,
	)
	if err != nil {
		return nil, err
	}
	for rows.Next() {
		var lang string
		if err := rows.Scan(&lang); err != nil {
			rows.Close()
			return nil, err
		}
		affectedLangs = append(affectedLangs, lang)
	}
	rows.Close()

	if _, err := tx.Exec(
		`UPDATE translations SET namespace = ? WHERE namespace = ?`,
		newNS, oldNS,
	); err != nil {
		return nil, err
	}
	if _, err := tx.Exec(
		`UPDATE metadata SET namespace = ? WHERE namespace = ?`,
		newNS, oldNS,
	); err != nil {
		return nil, err
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return affectedLangs, nil
}

// DeleteNamespace wipes every translation + metadata row whose namespace is
// `ns`, across every language. Returns the list of languages that had rows
// so the caller can clean up on-disk JSON artefacts.
func (d *DB) DeleteNamespace(ns string) (affectedLangs []string, err error) {
	if ns == "" {
		return nil, fmt.Errorf("namespace is required")
	}

	tx, err := d.conn.Begin()
	if err != nil {
		return nil, err
	}
	defer tx.Rollback() //nolint:errcheck

	rows, err := tx.Query(
		`SELECT DISTINCT lang FROM translations WHERE namespace = ?`, ns,
	)
	if err != nil {
		return nil, err
	}
	for rows.Next() {
		var lang string
		if err := rows.Scan(&lang); err != nil {
			rows.Close()
			return nil, err
		}
		affectedLangs = append(affectedLangs, lang)
	}
	rows.Close()

	if _, err := tx.Exec(
		`DELETE FROM translations WHERE namespace = ?`, ns,
	); err != nil {
		return nil, err
	}
	if _, err := tx.Exec(
		`DELETE FROM metadata WHERE namespace = ?`, ns,
	); err != nil {
		return nil, err
	}
	if err := tx.Commit(); err != nil {
		return nil, err
	}
	return affectedLangs, nil
}

// BulkSetTranslations upserts many rows in a single transaction.
func (d *DB) BulkSetTranslations(lang, namespace string, kv map[string]string) error {
	tx, err := d.conn.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback() //nolint:errcheck

	stmt, err := tx.Prepare(`
		INSERT INTO translations (lang, namespace, key, value)
		VALUES (?, ?, ?, ?)
		ON CONFLICT(lang, namespace, key) DO UPDATE SET
			value = excluded.value,
			updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
	`)
	if err != nil {
		return err
	}
	defer stmt.Close()

	for k, v := range kv {
		if _, err := stmt.Exec(lang, namespace, k, v); err != nil {
			return err
		}
	}
	return tx.Commit()
}

// ---------------------------------------------------------------------------
// Metadata
// ---------------------------------------------------------------------------

// GetMetadata returns the metadata row for a single (lang, namespace, key).
func (d *DB) GetMetadata(lang, namespace, key string) (KeyMetadata, bool, error) {
	var m KeyMetadata
	err := d.conn.QueryRow(`
		SELECT confidence, translated_by, timestamp, context
		FROM metadata WHERE lang = ? AND namespace = ? AND key = ?
	`, lang, namespace, key).Scan(&m.Confidence, &m.TranslatedBy, &m.Timestamp, &m.Context)
	if err == sql.ErrNoRows {
		return KeyMetadata{}, false, nil
	}
	if err != nil {
		return KeyMetadata{}, false, err
	}
	return m, true, nil
}

// GetAllMetadata returns every metadata row for (lang, namespace).
func (d *DB) GetAllMetadata(lang, namespace string) (map[string]KeyMetadata, error) {
	rows, err := d.conn.Query(`
		SELECT key, confidence, translated_by, timestamp, context
		FROM metadata WHERE lang = ? AND namespace = ?
	`, lang, namespace)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	out := make(map[string]KeyMetadata)
	for rows.Next() {
		var k string
		var m KeyMetadata
		if err := rows.Scan(&k, &m.Confidence, &m.TranslatedBy, &m.Timestamp, &m.Context); err != nil {
			return nil, err
		}
		out[k] = m
	}
	return out, rows.Err()
}

// LowConfidenceKeys returns keys in (lang, namespace) whose metadata
// confidence is strictly less than threshold. Keys without metadata are not
// reported.
func (d *DB) LowConfidenceKeys(lang, namespace string, threshold float64) ([]string, error) {
	rows, err := d.conn.Query(`
		SELECT key FROM metadata
		WHERE lang = ? AND namespace = ? AND confidence < ?
	`, lang, namespace, threshold)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []string
	for rows.Next() {
		var k string
		if err := rows.Scan(&k); err != nil {
			return nil, err
		}
		out = append(out, k)
	}
	return out, rows.Err()
}

// SetMetadata inserts-or-updates a single metadata row.
func (d *DB) SetMetadata(lang, namespace, key string, m KeyMetadata) error {
	_, err := d.conn.Exec(`
		INSERT INTO metadata (lang, namespace, key, confidence, translated_by, timestamp, context)
		VALUES (?, ?, ?, ?, ?, ?, ?)
		ON CONFLICT(lang, namespace, key) DO UPDATE SET
			confidence    = excluded.confidence,
			translated_by = excluded.translated_by,
			timestamp     = excluded.timestamp,
			context       = excluded.context
	`, lang, namespace, key, m.Confidence, m.TranslatedBy, m.Timestamp, m.Context)
	return err
}

// BulkSetMetadata upserts many metadata rows in a single transaction.
func (d *DB) BulkSetMetadata(lang, namespace string, entries map[string]KeyMetadata) error {
	tx, err := d.conn.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback() //nolint:errcheck

	stmt, err := tx.Prepare(`
		INSERT INTO metadata (lang, namespace, key, confidence, translated_by, timestamp, context)
		VALUES (?, ?, ?, ?, ?, ?, ?)
		ON CONFLICT(lang, namespace, key) DO UPDATE SET
			confidence    = excluded.confidence,
			translated_by = excluded.translated_by,
			timestamp     = excluded.timestamp,
			context       = excluded.context
	`)
	if err != nil {
		return err
	}
	defer stmt.Close()

	for k, m := range entries {
		if _, err := stmt.Exec(lang, namespace, k, m.Confidence, m.TranslatedBy, m.Timestamp, m.Context); err != nil {
			return err
		}
	}
	return tx.Commit()
}

// ---------------------------------------------------------------------------
// Config KV — unchanged from v1, namespace-agnostic.
// ---------------------------------------------------------------------------

// GetConfigSection reads the JSON blob for `key` and unmarshals it into `out`.
func (d *DB) GetConfigSection(key string, out any) (bool, error) {
	var raw string
	err := d.conn.QueryRow(`SELECT value_json FROM config_kv WHERE key = ?`, key).Scan(&raw)
	if err == sql.ErrNoRows {
		return false, nil
	}
	if err != nil {
		return false, err
	}
	if err := json.Unmarshal([]byte(raw), out); err != nil {
		return false, fmt.Errorf("unmarshal config_kv[%s]: %w", key, err)
	}
	return true, nil
}

// SetConfigSection marshals `value` to JSON and upserts it at `key`.
func (d *DB) SetConfigSection(key string, value any) error {
	raw, err := json.Marshal(value)
	if err != nil {
		return fmt.Errorf("marshal config_kv[%s]: %w", key, err)
	}
	_, err = d.conn.Exec(`
		INSERT INTO config_kv (key, value_json)
		VALUES (?, ?)
		ON CONFLICT(key) DO UPDATE SET
			value_json = excluded.value_json,
			updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
	`, key, string(raw))
	return err
}

// ---------------------------------------------------------------------------
// Glossary
// ---------------------------------------------------------------------------

// ListGlossary returns every glossary entry matching the given filters.
// Pass empty strings to skip a filter (e.g. ListGlossary("", "") for all).
func (d *DB) ListGlossary(sourceLang, targetLang string) ([]GlossaryEntry, error) {
	var (
		where = []string{}
		args  = []any{}
	)
	if sourceLang != "" {
		where = append(where, "source_lang = ?")
		args = append(args, sourceLang)
	}
	if targetLang != "" {
		where = append(where, "target_lang = ?")
		args = append(args, targetLang)
	}
	q := `SELECT id, source_lang, target_lang, source_term, target_term, target_variants, note, created_at, updated_at
	      FROM glossary`
	if len(where) > 0 {
		q += " WHERE "
		for i, w := range where {
			if i > 0 {
				q += " AND "
			}
			q += w
		}
	}
	q += " ORDER BY source_lang, target_lang, source_term COLLATE NOCASE"

	rows, err := d.conn.Query(q, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var out []GlossaryEntry
	for rows.Next() {
		var (
			e        GlossaryEntry
			variants string
		)
		if err := rows.Scan(&e.ID, &e.SourceLang, &e.TargetLang, &e.SourceTerm, &e.TargetTerm, &variants, &e.Note, &e.CreatedAt, &e.UpdatedAt); err != nil {
			return nil, err
		}
		e.TargetVariants = decodeGlossaryVariants(variants)
		out = append(out, e)
	}
	return out, rows.Err()
}

// AddGlossaryEntry inserts a new entry. Returns the auto-assigned id.
// Uniqueness is enforced on (source_lang, target_lang, LOWER(source_term)).
func (d *DB) AddGlossaryEntry(e GlossaryEntry) (int64, error) {
	res, err := d.conn.Exec(`
		INSERT INTO glossary (source_lang, target_lang, source_term, source_term_lower, target_term, target_variants, note)
		VALUES (?, ?, ?, ?, ?, ?, ?)
	`, e.SourceLang, e.TargetLang, e.SourceTerm, strings.ToLower(e.SourceTerm), e.TargetTerm, encodeGlossaryVariants(e.TargetVariants), e.Note)
	if err != nil {
		return 0, err
	}
	return res.LastInsertId()
}

// UpdateGlossaryEntry overwrites the source_term / target_term / variants /
// note for an existing id. source_lang and target_lang are treated as
// immutable — deleting and re-adding is cheaper than rewriting them here.
func (d *DB) UpdateGlossaryEntry(id int64, e GlossaryEntry) error {
	_, err := d.conn.Exec(`
		UPDATE glossary
		SET source_term = ?, source_term_lower = ?, target_term = ?, target_variants = ?, note = ?,
		    updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
		WHERE id = ?
	`, e.SourceTerm, strings.ToLower(e.SourceTerm), e.TargetTerm, encodeGlossaryVariants(e.TargetVariants), e.Note, id)
	return err
}

// encodeGlossaryVariants serializes the string slice into the newline-joined
// storage form used by the `target_variants` column. Empty/whitespace-only
// entries are dropped so round-tripping is idempotent.
func encodeGlossaryVariants(vs []string) string {
	cleaned := make([]string, 0, len(vs))
	for _, v := range vs {
		if t := strings.TrimSpace(v); t != "" {
			cleaned = append(cleaned, t)
		}
	}
	return strings.Join(cleaned, "\n")
}

// decodeGlossaryVariants is the inverse of encodeGlossaryVariants. Returns nil
// (not an empty slice) for the empty string so callers can cheap-check with
// `len(v) == 0`.
func decodeGlossaryVariants(s string) []string {
	if s == "" {
		return nil
	}
	parts := strings.Split(s, "\n")
	out := make([]string, 0, len(parts))
	for _, p := range parts {
		if t := strings.TrimSpace(p); t != "" {
			out = append(out, t)
		}
	}
	if len(out) == 0 {
		return nil
	}
	return out
}

// DeleteGlossaryEntry removes a single entry.
func (d *DB) DeleteGlossaryEntry(id int64) error {
	_, err := d.conn.Exec(`DELETE FROM glossary WHERE id = ?`, id)
	return err
}

// DeleteGlossaryEntriesForLang removes every glossary entry where `lang`
// appears on either side of the pair. Used when a language is removed from
// the app — the rules are meaningless without their language.
// Returns the number of rows deleted.
func (d *DB) DeleteGlossaryEntriesForLang(lang string) (int64, error) {
	if lang == "" {
		return 0, fmt.Errorf("lang is required")
	}
	res, err := d.conn.Exec(
		`DELETE FROM glossary WHERE source_lang = ? OR target_lang = ?`,
		lang, lang,
	)
	if err != nil {
		return 0, err
	}
	return res.RowsAffected()
}
