// API Service. All translation endpoints accept an optional `namespace`
// argument; when omitted the server default ("translation") is used.

// Mirrors backend `splitLocizeKey` / `isValidNamespaceIdent` so import preview
// works when the server binary predates `POST /api/import/preview` (404).
function isValidNamespaceIdentForImport(s) {
    return typeof s === 'string' && s.length > 0 && /^[A-Za-z0-9._-]+$/.test(s);
}

function splitLocizeKeyForImport(raw, defaultNS) {
    const idx = raw.indexOf(':');
    if (idx <= 0) return { ns: defaultNS, realKey: raw };
    const prefix = raw.slice(0, idx);
    if (!isValidNamespaceIdentForImport(prefix)) return { ns: defaultNS, realKey: raw };
    const rest = raw.slice(idx + 1);
    if (!rest || rest[0] === '/') return { ns: defaultNS, realKey: raw };
    return { ns: prefix, realKey: rest };
}

function truncateImportPreviewClient(s, maxRunes = 120) {
    const t = String(s ?? '').trim();
    if (!t) return '';
    const runes = [...t];
    if (runes.length <= maxRunes) return t;
    return runes.slice(0, maxRunes).join('') + '…';
}

async function buildImportPreviewClientSide(file, defaultNS) {
    const text = await file.text();
    let obj;
    try {
        obj = JSON.parse(text);
    } catch {
        throw new Error('Invalid JSON');
    }
    if (!obj || typeof obj !== 'object' || Array.isArray(obj)) {
        throw new Error('JSON must be a flat object of string keys to string values');
    }
    const translations = {};
    for (const [k, v] of Object.entries(obj)) {
        if (typeof v !== 'string') {
            throw new Error(`Non-string value for key "${k}" — use a flat map like {"key": "text"}`);
        }
        translations[k] = v;
    }
    const keys = Object.keys(translations).sort();
    const byNS = {};
    const sample_rows = [];
    const maxSamples = 50;
    for (const key of keys) {
        const value = translations[key];
        const { ns, realKey } = splitLocizeKeyForImport(key, defaultNS);
        byNS[ns] = (byNS[ns] || 0) + 1;
        if (sample_rows.length < maxSamples) {
            sample_rows.push({
                namespace: ns,
                key: realKey,
                raw_key: key,
                value_preview: truncateImportPreviewClient(value),
            });
        }
    }
    const namespaces = Object.keys(byNS)
        .sort()
        .map(namespace => ({ namespace, count: byNS[namespace] }));
    return {
        default_namespace: defaultNS,
        count: keys.length,
        namespaces,
        sample_rows,
    };
}

class APIService {
    constructor(baseURL = '/api') {
        this.baseURL = baseURL;
    }

    async request(method, endpoint, data = null) {
        const options = {
            method,
            headers: {
                'Content-Type': 'application/json',
            },
        };

        if (data) {
            options.body = JSON.stringify(data);
        }

        try {
            const response = await fetch(`${this.baseURL}${endpoint}`, options);

            if (!response.ok) {
                const errorData = await response.json().catch(() => ({}));
                return {
                    ok: false,
                    status: response.status,
                    error: errorData.error || `${response.status} ${response.statusText}`,
                };
            }

            const result = await response.json();
            return {
                ok: true,
                data: result,
            };
        } catch (error) {
            console.error('API Request Error:', error);
            return {
                ok: false,
                status: 0,
                error: error.message || 'Network error',
            };
        }
    }

    // Serialize an object to a `?k=v&...` string; ignores null / undefined / empty values.
    _qs(params) {
        const parts = [];
        for (const [k, v] of Object.entries(params || {})) {
            if (v === undefined || v === null || v === '') continue;
            parts.push(`${encodeURIComponent(k)}=${encodeURIComponent(v)}`);
        }
        return parts.length ? `?${parts.join('&')}` : '';
    }

    async health() {
        return this.request('GET', '/health');
    }

    async getLanguages() {
        return this.request('GET', '/languages');
    }

    async getNamespaces(lang = null) {
        return this.request('GET', `/namespaces${this._qs({ lang })}`);
    }

    // getNamespaceStats returns per-namespace key + translation counts summed
    // across every language. Used by the "Manage namespaces" modal so users
    // can see how populated each bucket is before renaming or deleting it.
    async getNamespaceStats() {
        return this.request('GET', `/namespaces/stats`);
    }

    // renameNamespace re-buckets every key from `from` to `to`, across every
    // language. Refuses to merge into a populated target (the backend
    // enforces this; we surface the error verbatim).
    async renameNamespace(from, to) {
        return this.request('PUT', `/namespaces/${encodeURIComponent(from)}`, { to });
    }

    // deleteNamespace wipes every key under `ns` across every language and
    // removes the on-disk JSON files. Hard delete — the "Manage namespaces"
    // modal gates this behind a confirm().
    async deleteNamespace(ns) {
        return this.request('DELETE', `/namespaces/${encodeURIComponent(ns)}`);
    }

    async getConfig() {
        return this.request('GET', '/config');
    }

    async updateConfig(config) {
        return this.request('POST', '/config', config);
    }

    async getTranslations(lang, namespace = null) {
        return this.request('GET', `/translations/${lang}${this._qs({ ns: namespace })}`);
    }

    async getTranslationStats(lang, sourceLang = 'en', namespace = null) {
        return this.request('GET', `/translations/${lang}/stats${this._qs({ source: sourceLang, ns: namespace })}`);
    }

    async getMissingTranslations(targetLang, sourceLang = 'en', namespace = null) {
        return this.request('GET', `/translations/${targetLang}/missing${this._qs({ source: sourceLang, ns: namespace })}`);
    }

    // getMissingSource returns keys that are empty in the *source* language
    // but filled in some other language — the Review page turns these into
    // back-translation rows (e.g. "the English is missing; use the Swedish
    // as the source"). Each item carries a `source_lang` so the caller
    // knows which direction to drive the LLM in.
    async getMissingSource(sourceLang, namespace = null) {
        return this.request('GET', `/translations/${sourceLang}/missing-source${this._qs({ ns: namespace })}`);
    }

    async getLowConfidenceTranslations(lang, threshold = 0.7, namespace = null) {
        return this.request('GET', `/translations/${lang}/low-confidence${this._qs({ threshold, ns: namespace })}`);
    }

    async updateTranslation(lang, key, translation, metadata = null, namespace = null) {
        return this.request('PUT', `/translations/${lang}/${encodeURIComponent(key)}${this._qs({ ns: namespace })}`, {
            translation,
            metadata,
        });
    }

    // deleteKey drops a (namespace, key) pair from every language at once and
    // re-exports the affected JSON files. Used by the trash button on each
    // Translations page row.
    async deleteKey(key, namespace = null) {
        return this.request('DELETE', `/keys${this._qs({ ns: namespace, key })}`);
    }

    // translateSingle runs the LLM for one (namespace, key, target) combo.
    // Pass dryRun=true to *preview* the translation without writing it — used
    // by the Review page's confirm flow (the user accepts or rejects the
    // proposal, and only then do we PUT it via updateTranslation).
    //
    // `sourceText` overrides the backend's DB lookup — the Translations page's
    // "✨ fill source from translation" button uses it to back-translate an
    // existing target value into the source language when the source cell is
    // empty (otherwise the backend would 400 with "key not found").
    async translateSingle(key, targetLang, sourceLang = 'en', namespace = null, dryRun = false, sourceText = null) {
        return this.request('POST', '/translate/single', {
            key,
            namespace: namespace || undefined,
            target_lang: targetLang,
            source_lang: sourceLang,
            dry_run: dryRun,
            source_text: sourceText || undefined,
        });
    }

    // deleteLanguage removes a language from the app: translations, metadata,
    // filesystem files, matching language_pairs, glossary entries and (if
    // applicable) the default_target_language preference are all cleared.
    // Use the `/languages/:lang` endpoint — the old `/translations/:lang`
    // alias still works on the server but this one is the canonical shape.
    async deleteLanguage(lang) {
        return this.request('DELETE', `/languages/${encodeURIComponent(lang)}`);
    }

    // addLanguage registers a new target language by creating a default
    // `{source}_{code}` language pair. After this call the language appears
    // in /api/languages (with 0 translations) so the UI can immediately show
    // and translate it. `model` is optional — leave empty to use whichever
    // LLM the backend picks as default.
    async addLanguage(code, model = '') {
        return this.request('POST', '/languages', { code, model });
    }

    // ---------------------------------------------------------------------
    // Glossary: preferred-term rules injected into LLM prompts so certain
    // words always translate consistently (e.g. "email" → "e-post" in sv).
    // ---------------------------------------------------------------------
    async listGlossary(sourceLang = null, targetLang = null) {
        return this.request('GET', `/glossary${this._qs({ source_lang: sourceLang, target_lang: targetLang })}`);
    }
    async addGlossaryEntry(entry) {
        return this.request('POST', '/glossary', entry);
    }
    async updateGlossaryEntry(id, entry) {
        return this.request('PUT', `/glossary/${id}`, entry);
    }
    async deleteGlossaryEntry(id) {
        return this.request('DELETE', `/glossary/${id}`);
    }

    // suggestGlossaryVariants asks the LLM configured for (source_lang,
    // target_lang) to list grammatical forms / inflections of target_term.
    // Used by the Glossary page's "✨ Find variants" button. Returns
    // { variants: [...] }; an empty list means "model had nothing useful" —
    // not an error, just type them manually.
    async suggestGlossaryVariants({ source_lang, target_lang, source_term, target_term }) {
        return this.request('POST', '/glossary/suggest-variants', {
            source_lang, target_lang, source_term, target_term,
        });
    }

    async translateMissing(targetLangs, sourceLang = 'en', namespace = null) {
        return this.request('POST', '/translate/missing', {
            target_langs: targetLangs,
            source_lang: sourceLang,
            namespace: namespace || undefined,
        });
    }

    // importPreview runs the same JSON/locize parsing as POST /import but does
    // not write — used by the Import page review step.
    // If the server returns 404 (older binary without /import/preview), we
    // parse the file in the browser using the same rules as the Go importer.
    async importPreview(file, namespace = null) {
        const defaultNS = (namespace && String(namespace).trim()) ? String(namespace).trim() : 'translation';
        const formData = new FormData();
        formData.append('file', file);
        formData.append('namespace', defaultNS);
        try {
            let response = await fetch(`${this.baseURL}/import/preview`, {
                method: 'POST',
                body: formData,
            });
            if (response.status === 404) {
                response = await fetch(`${this.baseURL}/import-preview`, {
                    method: 'POST',
                    body: formData,
                });
            }
            if (response.ok) {
                const data = await response.json();
                return { ok: true, data };
            }
            if (response.status === 404) {
                try {
                    const data = await buildImportPreviewClientSide(file, defaultNS);
                    console.info('Import preview: using client-side fallback (server has no /api/import/preview). Rebuild the backend to use server preview.');
                    return { ok: true, data, clientPreview: true };
                } catch (fallbackErr) {
                    console.error('Import preview client fallback:', fallbackErr);
                    return {
                        ok: false,
                        status: 404,
                        error: `Server has no preview endpoint (404). ${fallbackErr.message || fallbackErr}. Rebuild/restart the app, or fix the JSON file.`,
                    };
                }
            }
            const errorData = await response.json().catch(() => ({}));
            return {
                ok: false,
                status: response.status,
                error: errorData.error || `${response.status} ${response.statusText}`,
            };
        } catch (error) {
            console.error('Import preview error:', error);
            return { ok: false, status: 0, error: error.message || 'Preview failed' };
        }
    }

    async importTranslations(file, lang, mode = 'merge', namespace = null) {
        const formData = new FormData();
        formData.append('file', file);
        formData.append('lang', lang);
        formData.append('mode', mode);
        if (namespace) formData.append('namespace', namespace);

        try {
            const response = await fetch(`${this.baseURL}/import`, {
                method: 'POST',
                body: formData,
            });

            if (!response.ok) {
                const errorData = await response.json().catch(() => ({}));
                return {
                    ok: false,
                    status: response.status,
                    error: errorData.error || `${response.status} ${response.statusText}`,
                };
            }

            const result = await response.json();
            return {
                ok: true,
                data: result,
            };
        } catch (error) {
            console.error('Import Error:', error);
            return {
                ok: false,
                status: 0,
                error: error.message || 'Import failed',
            };
        }
    }

    async search(query, lang = null) {
        return this.request('GET', `/search${this._qs({ q: query, lang })}`);
    }

    async getMetadata(lang, namespace = null) {
        return this.request('GET', `/metadata/${lang}${this._qs({ ns: namespace })}`);
    }

    async updateMetadata(lang, key, metadata, namespace = null) {
        return this.request('PUT', `/metadata/${lang}/${encodeURIComponent(key)}${this._qs({ ns: namespace })}`, metadata);
    }

    async testLLM(endpoint) {
        return this.request('GET', `/llm/test${this._qs({ endpoint })}`);
    }

    async getLLMStatus(name) {
        return this.request('GET', `/llm/status${this._qs({ name })}`);
    }

    async startLLM(name) {
        return this.request('POST', '/llm/start', { name });
    }

    async stopLLM(name) {
        return this.request('POST', '/llm/stop', { name });
    }
}

const api = new APIService();
