// Glossary page logic: list, add, edit, delete preferred-term rules that get
// injected into the LLM prompt by the translation service.
let currentConfig = null;
let allLanguages = [];

const glossaryState = {
    entries: [],
    pairFilter: '',
    editingId: null,
    // Live chip-input instances keyed by a stable id: "add" for the add-form
    // mount (created once in initGlossary), and "edit-<entryId>" for any row
    // currently in edit mode (recreated on every renderGlossary).
    chipInputs: new Map(),
};

// mountChipInput upgrades a plain container <div> into a tag/pill input
// — returns a handle with { getValues, setValues, focus, clear }.
//
// Keyboard:
//   Enter / Tab / "," → commit the current draft as a chip
//   Backspace (empty) → remove the last chip
//   Paste             → commits each comma- or newline-separated piece
//
// Duplicates (case-insensitive) are skipped. Whitespace is trimmed.
function mountChipInput(container, initialValues, placeholder) {
    const values = parseVariantsCsv(
        Array.isArray(initialValues) ? initialValues.join(',') : (initialValues || '')
    );

    // The box looks like a field (ui-field) and rings on focus-within.
    container.classList.add('chip-input', 'ui-field');
    container.setAttribute('role', 'group');
    container.innerHTML = '';

    const input = document.createElement('input');
    input.type = 'text';
    input.className = 'chip-input-text';
    input.placeholder = placeholder || 'Type and press Enter';
    if (container.dataset.inputId) input.id = container.dataset.inputId;
    else input.setAttribute('aria-label', 'Accepted forms');
    container.appendChild(input);

    function render() {
        container.querySelectorAll('.chip').forEach(c => c.remove());
        values.forEach((v, i) => {
            const chip = document.createElement('span');
            chip.className = 'ui-chip chip';
            chip.innerHTML = `<span>${esc(v)}</span>`;
            const x = document.createElement('button');
            x.type = 'button';
            x.className = 'ui-button ui-icon-button chip-x';
            x.setAttribute('aria-label', `Remove ${v}`);
            x.innerHTML = icon('x', 'icon-sm');
            x.addEventListener('click', (e) => {
                e.stopPropagation();
                values.splice(i, 1);
                render();
                input.focus();
            });
            chip.appendChild(x);
            container.insertBefore(chip, input);
        });
    }

    function addRaw(raw) {
        const parts = parseVariantsCsv(raw);
        let added = 0;
        for (const p of parts) {
            if (!values.some(v => v.toLowerCase() === p.toLowerCase())) {
                values.push(p);
                added++;
            }
        }
        return added;
    }

    function commitDraft() {
        if (!input.value) return;
        addRaw(input.value);
        input.value = '';
        render();
    }

    input.addEventListener('keydown', (e) => {
        if (e.key === 'Enter' || e.key === 'Tab' || e.key === ',') {
            if (input.value.trim()) {
                e.preventDefault();
                commitDraft();
            } else if (e.key === ',') {
                e.preventDefault();
            }
        } else if (e.key === 'Backspace' && input.value === '' && values.length > 0) {
            e.preventDefault();
            values.pop();
            render();
        }
    });

    input.addEventListener('blur', commitDraft);

    input.addEventListener('paste', (e) => {
        const text = (e.clipboardData || window.clipboardData)?.getData('text') || '';
        if (/[,\n\r]/.test(text)) {
            e.preventDefault();
            addRaw(text);
            render();
        }
    });

    container.addEventListener('mousedown', (e) => {
        // Clicking the padding (not a chip or its x) focuses the text input.
        if (e.target === container) {
            e.preventDefault();
            input.focus();
        }
    });

    render();

    return {
        getValues() { commitDraft(); return [...values]; },
        setValues(next) {
            values.length = 0;
            for (const v of parseVariantsCsv(Array.isArray(next) ? next.join(',') : next)) {
                values.push(v);
            }
            render();
        },
        focus() { input.focus(); },
        clear() { values.length = 0; input.value = ''; render(); },
    };
}

document.addEventListener('DOMContentLoaded', async () => {
    await initGlossaryPage();
    setupKeyboardShortcuts();
});

async function initGlossaryPage() {
    try {
        const [configResult, langResult] = await Promise.all([
            api.getConfig(),
            api.getLanguages(),
        ]);

        if (!configResult.ok) {
            Utils.showToast('Failed to load configuration', 'error');
            return;
        }
        currentConfig = configResult.data;

        if (!langResult.ok) {
            Utils.showToast('Failed to load languages', 'error');
            return;
        }
        allLanguages = (langResult.data || []).map(l => l.code || l);

        initGlossary();
    } catch (err) {
        console.error('initGlossaryPage:', err);
        Utils.showToast('Error initializing glossary', 'error');
    }
}

function initGlossary() {
    const sourceSel = document.getElementById('newGlossarySource');
    const targetSel = document.getElementById('newGlossaryTarget');
    const filterSel = document.getElementById('glossaryPairFilter');

    for (const code of allLanguages) {
        sourceSel.appendChild(makeOpt(code));
        targetSel.appendChild(makeOpt(code));
    }
    // Pre-select source = config source language, target = default target.
    sourceSel.value = currentConfig.user_preferences.source_language;
    const preTgt = currentConfig.user_preferences.default_target_language;
    if ([...targetSel.options].some(o => o.value === preTgt)) targetSel.value = preTgt;

    // Pair filter lists configured language pairs so the user only sees
    // directions that actually make sense in this project.
    const pairs = Object.keys(currentConfig.language_pairs || {}).sort();
    for (const k of pairs) {
        const [s, t] = k.split('_');
        const opt = document.createElement('option');
        opt.value = k;
        opt.textContent = `${langName(s)} → ${langName(t)}`;
        filterSel.appendChild(opt);
    }

    // Upgrade the "Accepted forms" placeholder div into a real chip input.
    const addChipMount = document.getElementById('newGlossaryVariants');
    if (addChipMount) {
        glossaryState.chipInputs.set('add', mountChipInput(
            addChipMount, [], 'e-posten, e-poster…'
        ));
    }

    loadGlossary();

    document.getElementById('addGlossaryBtn').addEventListener('click', addGlossary);
    filterSel.addEventListener('change', (e) => {
        glossaryState.pairFilter = e.target.value;
        renderGlossary();
    });

    const findBtn = document.getElementById('newGlossaryFindVariantsBtn');
    if (findBtn) {
        findBtn.addEventListener('click', () => findVariantsForAddForm(findBtn));
    }
}

function makeOpt(code) {
    const o = document.createElement('option');
    o.value = code;
    o.textContent = langName(code);
    return o;
}

// Language name for a code, as the Dashboard shows it (no flags).
function langName(code) {
    return Utils.langInfo(code).name;
}

async function loadGlossary() {
    try {
        const r = await api.listGlossary();
        if (!r.ok) {
            Utils.showToast(`Failed to load glossary: ${r.error}`, 'error');
            glossaryState.entries = [];
        } else {
            glossaryState.entries = r.data.entries || [];
        }
        renderGlossary();
    } catch (err) {
        console.error('loadGlossary:', err);
        Utils.showToast('Error loading glossary', 'error');
    }
}

function renderGlossary() {
    const container = document.getElementById('glossaryList');
    const countEl = document.getElementById('glossaryCount');
    const entries = glossaryState.entries.filter(e => {
        if (!glossaryState.pairFilter) return true;
        return `${e.source_lang}_${e.target_lang}` === glossaryState.pairFilter;
    });
    // The lead says what the state needs: with no terms at all, that is
    // the first one; the list then stays empty rather than saying it twice.
    const none = glossaryState.entries.length === 0;
    if (countEl) countEl.textContent = none ? 'No' : String(entries.length);
    const nounEl = document.getElementById('glossaryCountNoun');
    if (nounEl) nounEl.textContent = none ? 'preferred terms yet' : (entries.length === 1 ? 'preferred term' : 'preferred terms');

    // Throw away edit-mode chip-input handles from the previous render; their
    // DOM nodes are about to be destroyed. Keep the "add" handle — its node
    // lives outside this container.
    for (const k of [...glossaryState.chipInputs.keys()]) {
        if (k !== 'add') glossaryState.chipInputs.delete(k);
    }

    if (entries.length === 0) {
        container.innerHTML = none
            ? ''
            : '<p class="empty">No terms for this pair. Pick All pairs, or add one above.</p>';
        return;
    }

    // Group by source→target pair for readability.
    const groups = new Map();
    for (const e of entries) {
        const k = `${e.source_lang}_${e.target_lang}`;
        if (!groups.has(k)) groups.set(k, []);
        groups.get(k).push(e);
    }

    container.innerHTML = [...groups.entries()].map(([pair, items]) => {
        const [src, tgt] = pair.split('_');
        return `
            <section class="gl-group">
                <p class="group-label">${esc(langName(src))} → ${esc(langName(tgt))} · ${items.length}</p>
                <div class="list">
                    ${items.map(renderGlossaryRow).join('')}
                </div>
            </section>
        `;
    }).join('');

    // Upgrade every edit-row's "Accepted forms" placeholder into a chip input.
    container.querySelectorAll('[data-chipinput-id]').forEach(mount => {
        const id = mount.dataset.chipinputId;
        const initial = mount.dataset.initial ? mount.dataset.initial.split('\u0001').filter(Boolean) : [];
        glossaryState.chipInputs.set(id, mountChipInput(
            mount, initial, 'Add a variant, press Enter'
        ));
    });

    container.querySelectorAll('[data-glossary-edit]').forEach(btn => {
        btn.addEventListener('click', () => startEditGlossary(parseInt(btn.dataset.glossaryEdit, 10)));
    });
    container.querySelectorAll('[data-glossary-delete]').forEach(btn => {
        btn.addEventListener('click', () => deleteGlossary(parseInt(btn.dataset.glossaryDelete, 10)));
    });
    container.querySelectorAll('[data-glossary-save]').forEach(btn => {
        btn.addEventListener('click', () => commitEditGlossary(parseInt(btn.dataset.glossarySave, 10)));
    });
    container.querySelectorAll('[data-glossary-cancel]').forEach(btn => {
        btn.addEventListener('click', () => { glossaryState.editingId = null; renderGlossary(); });
    });
    container.querySelectorAll('[data-glossary-find-variants]').forEach(btn => {
        btn.addEventListener('click', () => findVariantsForEditRow(parseInt(btn.dataset.glossaryFindVariants, 10), btn));
    });
}

function renderGlossaryRow(e) {
    const editing = glossaryState.editingId === e.id;
    const variants = Array.isArray(e.target_variants) ? e.target_variants : [];
    const variantsCsv = variants.join(', ');

    if (editing) {
        // The target_variants cell is a mount point — renderGlossary() upgrades
        // it into a live chip input once the HTML is in the DOM. We smuggle
        // the initial values through `data-initial`, joined with \u0001 so
        // commas inside variants survive round-tripping (unlikely but safe).
        const initialAttr = variants.join('\u0001');
        return `
            <div class="gl-row gl-row-editing ui-row is-selected">
                <input type="text" value="${esc(e.source_term)}" data-edit-field="source_term" data-entry="${e.id}"
                    class="ui-field input w-full" aria-label="Source term">
                <input type="text" value="${esc(e.target_term)}" data-edit-field="target_term" data-entry="${e.id}"
                    class="ui-field input w-full" aria-label="Target term">
                <div class="gl-row-forms">
                    <div data-chipinput-id="edit-${e.id}" data-initial="${esc(initialAttr)}"
                         title="Accepted forms the validator should allow as equivalent to the target term (grammatical inflections, plurals, definite forms, …)."></div>
                    <button type="button" class="ui-button find-variants-btn" data-glossary-find-variants="${e.id}"
                            title="Ask the LLM to suggest grammatical forms of the target term">
                        ${icon('sparkles')}Find variants
                    </button>
                </div>
                <input type="text" value="${esc(e.note || '')}" placeholder="Note" data-edit-field="note" data-entry="${e.id}"
                    class="ui-field input w-full" aria-label="Note">
                <div class="gl-row-actions">
                    <button type="button" data-glossary-save="${e.id}" class="ui-button">Save</button>
                    <button type="button" data-glossary-cancel="${e.id}" class="ui-button">Cancel</button>
                </div>
            </div>
        `;
    }

    const variantsChips = variants.length
        ? `<div class="chip-list">${variants.map(v =>
                `<span class="ui-chip">${esc(v)}</span>`
            ).join('')}</div>`
        : '';

    return `
        <div class="gl-row ui-row">
            <div class="gl-term truncate" title="${esc(e.source_term)}">${esc(e.source_term)}</div>
            <div class="gl-term truncate" title="${esc(e.target_term)}"><span class="text-faint" aria-hidden="true">→ </span>${esc(e.target_term)}</div>
            <div class="min-w-0" title="${esc(variantsCsv)}">${variantsChips}</div>
            <div class="gl-note truncate" title="${esc(e.note || '')}">${esc(e.note || '')}</div>
            <div class="gl-row-actions gl-row-quiet">
                <button type="button" data-glossary-edit="${e.id}" class="ui-button ui-icon-button" aria-label="Edit ${esc(e.source_term)}" title="Edit">${icon('pencil')}</button>
                <button type="button" data-glossary-delete="${e.id}" class="ui-button ui-icon-button" aria-label="Remove ${esc(e.source_term)}" title="Remove">${icon('trash')}</button>
            </div>
        </div>
    `;
}

// parseVariantsCsv normalizes the comma-separated "accepted forms" input into
// a deduped, trimmed array of non-empty strings. Case is preserved so the
// prompt shows variants exactly as the reviewer typed them.
function parseVariantsCsv(raw) {
    if (!raw) return [];
    const seen = new Set();
    const out = [];
    for (const piece of raw.split(',')) {
        const t = piece.trim();
        if (!t) continue;
        const key = t.toLowerCase();
        if (seen.has(key)) continue;
        seen.add(key);
        out.push(t);
    }
    return out;
}

function startEditGlossary(id) {
    glossaryState.editingId = id;
    renderGlossary();
    const first = document.querySelector(`[data-edit-field="source_term"][data-entry="${id}"]`);
    if (first) first.focus();
}

async function commitEditGlossary(id) {
    const sourceTerm = document.querySelector(`[data-edit-field="source_term"][data-entry="${id}"]`).value.trim();
    const targetTerm = document.querySelector(`[data-edit-field="target_term"][data-entry="${id}"]`).value.trim();
    const chip = glossaryState.chipInputs.get(`edit-${id}`);
    const variants = chip ? chip.getValues() : [];
    const note = document.querySelector(`[data-edit-field="note"][data-entry="${id}"]`).value.trim();
    if (!sourceTerm || !targetTerm) {
        Utils.showToast('Source and target term are required', 'error');
        return;
    }
    const r = await api.updateGlossaryEntry(id, {
        source_term: sourceTerm,
        target_term: targetTerm,
        target_variants: variants,
        note,
    });
    if (!r.ok) {
        Utils.showToast(`Update failed: ${r.error}`, 'error');
        return;
    }
    Utils.showToast('Term updated', 'success');
    glossaryState.editingId = null;
    await loadGlossary();
}

async function deleteGlossary(id) {
    const entry = glossaryState.entries.find(x => x.id === id);
    if (!entry) return;
    const ok = await confirmDialog({
        title: `Remove “${entry.source_term}”?`,
        body: `The ${langName(entry.source_lang)} → ${langName(entry.target_lang)} rule “${entry.source_term}” → “${entry.target_term}” stops applying to new translations.`,
        confirmLabel: 'Remove term',
    });
    if (!ok) return;
    const r = await api.deleteGlossaryEntry(id);
    if (!r.ok) {
        Utils.showToast(`Delete failed: ${r.error}`, 'error');
        return;
    }
    Utils.showToast('Term removed', 'success');
    await loadGlossary();
}

async function addGlossary() {
    const addChip = glossaryState.chipInputs.get('add');
    const payload = {
        source_lang: document.getElementById('newGlossarySource').value,
        target_lang: document.getElementById('newGlossaryTarget').value,
        source_term: document.getElementById('newGlossarySourceTerm').value.trim(),
        target_term: document.getElementById('newGlossaryTargetTerm').value.trim(),
        target_variants: addChip ? addChip.getValues() : [],
        note:        document.getElementById('newGlossaryNote').value.trim(),
    };
    if (!payload.source_lang || !payload.target_lang) {
        Utils.showToast('Pick source and target languages', 'error'); return;
    }
    if (payload.source_lang === payload.target_lang) {
        Utils.showToast('Source and target must differ', 'error'); return;
    }
    if (!payload.source_term || !payload.target_term) {
        Utils.showToast('Both terms are required', 'error'); return;
    }

    const r = await api.addGlossaryEntry(payload);
    if (!r.ok) {
        Utils.showToast(`Add failed: ${r.error}`, 'error'); return;
    }
    Utils.showToast('Added to the glossary', 'success');
    document.getElementById('newGlossarySourceTerm').value = '';
    document.getElementById('newGlossaryTargetTerm').value = '';
    document.getElementById('newGlossaryNote').value = '';
    if (addChip) addChip.clear();
    await loadGlossary();
}

// ------------------------------------------------------------------
// "Find variants" — ask the LLM for grammatical forms of the target term
// and merge them into the chip input. Used from both the Add form and from
// each row's edit mode; the shared core handles the busy state + API call,
// callers just tell it which chip input + which (src, tgt, srcTerm, tgtTerm)
// to work with.
// ------------------------------------------------------------------

async function findVariantsForAddForm(btn) {
    const sourceLang = document.getElementById('newGlossarySource').value;
    const targetLang = document.getElementById('newGlossaryTarget').value;
    const sourceTerm = document.getElementById('newGlossarySourceTerm').value.trim();
    const targetTerm = document.getElementById('newGlossaryTargetTerm').value.trim();

    if (!targetLang || !targetTerm) {
        Utils.showToast('Enter a target term first — the LLM needs something to inflect.', 'warning');
        return;
    }
    const chip = glossaryState.chipInputs.get('add');
    if (!chip) return;
    await runFindVariants(btn, chip, { sourceLang, targetLang, sourceTerm, targetTerm });
}

async function findVariantsForEditRow(id, btn) {
    const entry = glossaryState.entries.find(x => x.id === id);
    if (!entry) return;
    // The edit-mode inputs are what's ON SCREEN right now — the user may
    // have changed the target term but not clicked Save yet, and we want to
    // inflect what they've actually typed.
    const sourceTerm = document.querySelector(`[data-edit-field="source_term"][data-entry="${id}"]`)?.value.trim() || entry.source_term;
    const targetTerm = document.querySelector(`[data-edit-field="target_term"][data-entry="${id}"]`)?.value.trim() || entry.target_term;
    if (!targetTerm) {
        Utils.showToast('Enter a target term first.', 'warning');
        return;
    }
    const chip = glossaryState.chipInputs.get(`edit-${id}`);
    if (!chip) return;
    await runFindVariants(btn, chip, {
        sourceLang: entry.source_lang,
        targetLang: entry.target_lang,
        sourceTerm,
        targetTerm,
    });
}

async function runFindVariants(btn, chip, { sourceLang, targetLang, sourceTerm, targetTerm }) {
    btn.setAttribute('data-busy', 'true');
    btn.disabled = true;
    try {
        const r = await api.suggestGlossaryVariants({
            source_lang: sourceLang,
            target_lang: targetLang,
            source_term: sourceTerm,
            target_term: targetTerm,
        });
        if (!r.ok) {
            Utils.showToast(`Find variants failed: ${r.error}`, 'error');
            return;
        }
        const suggested = Array.isArray(r.data?.variants) ? r.data.variants : [];
        if (!suggested.length) {
            Utils.showToast('LLM had nothing to add — type variants manually if needed.', 'info');
            return;
        }

        // Merge: keep whatever the reviewer already has, add new ones at the
        // end. The chip input's setValues dedupes case-insensitively, so we
        // don't have to filter here.
        const existing = chip.getValues();
        const merged = [...existing];
        for (const v of suggested) {
            if (!merged.some(e => e.toLowerCase() === v.toLowerCase())) {
                merged.push(v);
            }
        }
        chip.setValues(merged);
        const added = merged.length - existing.length;
        Utils.showToast(`Added ${added} variant${added === 1 ? '' : 's'}`, 'success');
    } catch (err) {
        console.error('runFindVariants:', err);
        Utils.showToast('Unexpected error. See console.', 'error');
    } finally {
        btn.removeAttribute('data-busy');
        btn.disabled = false;
    }
}
function esc(s) {
    return String(s ?? '')
        .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}

function setupKeyboardShortcuts() {
    document.addEventListener('keydown', (e) => {
        const isMeta = e.metaKey || e.ctrlKey;
        if (isMeta && e.key === '1') { e.preventDefault(); window.location.href = '/'; }
        if (isMeta && e.key === '2') { e.preventDefault(); window.location.href = '/ui/translations.html'; }
        if (isMeta && e.key === '3') { e.preventDefault(); window.location.href = '/ui/missing.html'; }
        if (isMeta && e.key === '4') { e.preventDefault(); window.location.href = '/ui/glossary.html'; }
        if (isMeta && e.key === '5') { e.preventDefault(); window.location.href = '/ui/settings.html'; }
        if (isMeta && e.key === '6') { e.preventDefault(); window.location.href = '/ui/import.html'; }
    });
}
