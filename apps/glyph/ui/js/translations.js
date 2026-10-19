// Translations page: one group per namespace, one row per key.
//
// A group's header carries its progress per language; a long project starts
// as one line per namespace and the reader opens the one they work on. A key
// row shows its source text and a dot per language (done, missing, low
// confidence). Opening a row reveals every language as an editable row
// (name · input · confidence · actions).
//
// Shape of `state.rows`:
//   { ns, key, cells: { [lang]: { value, confidence, translatedBy, missing,
//                                 dirty, saving } } }
//
// Everything else — the Show filter, search, the language picker, open
// groups — is view state and recomputed on each render.

const state = {
    config: null,
    sourceLang: 'en',
    targetLangs: [],
    allLangs: [],       // [source, ...targets]
    namespaces: [],
    confidenceByPair: {},

    rows: [],

    search: '',
    langView: 'all',            // 'all' | <target lang code>
    show: 'all',                // all | missing | low
    open: new Set(),            // namespaces whose keys are shown
    expanded: new Set(),        // "ns::key"
    lastLoad: 0,
};

// Open groups survive a reload. Storage can be missing (private window,
// blocked site data), so every read and write is guarded.
const OPEN_KEY = 'glyph.translations.open';
function loadOpenGroups() {
    try {
        const raw = localStorage.getItem(OPEN_KEY);
        if (raw === null) return null;
        return new Set(JSON.parse(raw));
    } catch { return null; }
}
function saveOpenGroups() {
    try { localStorage.setItem(OPEN_KEY, JSON.stringify([...state.open])); } catch { /* view state only */ }
}

document.addEventListener('DOMContentLoaded', async () => {
    await init();
    setupToolbar();
    setupNewKeyModal();
    setupNsManageModal();
    setupKeyboardShortcuts();
    setupAutoReload();
});

async function init() {
    try {
        const [configRes, langRes, nsRes] = await Promise.all([
            api.getConfig(),
            api.getLanguages(),
            api.getNamespaces(),
        ]);
        if (!configRes.ok) {
            Utils.showToast('Failed to load configuration', 'error');
            return;
        }
        state.config = configRes.data;
        state.sourceLang = state.config.user_preferences.source_language || 'en';

        const codes = (langRes.ok ? langRes.data : [])
            .map(l => l.code || l).filter(Boolean);
        state.targetLangs = codes.filter(c => c !== state.sourceLang).sort();
        state.allLangs = [state.sourceLang, ...state.targetLangs];

        for (const tgt of state.targetLangs) {
            const pair = state.config.language_pairs?.[`${state.sourceLang}_${tgt}`];
            state.confidenceByPair[tgt] = pair?.confidence_threshold ?? 0.7;
        }

        state.namespaces = nsRes.ok ? (nsRes.data.namespaces || []) : [];

        // First visit: open the first namespace, fold the rest.
        const stored = loadOpenGroups();
        state.open = stored ?? new Set(state.namespaces.slice(0, 1));

        renderLangsMenu();
        updateToolbarLabels();

        await loadAll();
    } catch (err) {
        console.error('Init error:', err);
        Utils.showToast('Error loading translations', 'error');
    }
}

// loadAll fetches every (lang, ns) translation + metadata in parallel and
// folds them into state.rows keyed by (ns, key). A quiet load (coming back to
// the window) keeps the list on screen until the new rows are in.
async function loadAll({ quiet = false } = {}) {
    const list = document.getElementById('keysList');
    if (!quiet) Utils.showLoading(list, 'Loading translations…');
    state.lastLoad = Date.now();

    // Namespaces change under us: renamed or deleted here, added on disk.
    const nsRes = await api.getNamespaces();
    if (nsRes.ok) state.namespaces = nsRes.data.namespaces || [];
    const namespaces = state.namespaces.length ? state.namespaces : ['translation'];

    const tasks = [];
    for (const lang of state.allLangs) {
        for (const ns of namespaces) {
            tasks.push({ lang, ns });
        }
    }

    try {
        const transResults = await Promise.all(tasks.map(t => api.getTranslations(t.lang, t.ns)));
        const metaTasks = tasks.filter(t => t.lang !== state.sourceLang);
        const metaResults = await Promise.all(metaTasks.map(t => api.getMetadata(t.lang, t.ns)));
        const srcMetaResults = await Promise.all(namespaces.map(ns => api.getMetadata(state.sourceLang, ns)));

        const trans = {};
        tasks.forEach((t, i) => {
            trans[`${t.lang}|${t.ns}`] = transResults[i].ok ? (transResults[i].data || {}) : {};
        });
        const meta = {};
        metaTasks.forEach((t, i) => {
            meta[`${t.lang}|${t.ns}`] = metaResults[i].ok ? (metaResults[i].data?.metadata || {}) : {};
        });
        const srcMetaByNs = {};
        namespaces.forEach((ns, i) => {
            srcMetaByNs[ns] = srcMetaResults[i].ok ? (srcMetaResults[i].data?.metadata || {}) : {};
        });

        const rowMap = new Map();
        for (const ns of namespaces) {
            // Union of keys across every language so target-only keys still
            // show up (and we can see they're missing a source).
            for (const lang of state.allLangs) {
                const t = trans[`${lang}|${ns}`] || {};
                for (const k of Object.keys(t)) {
                    const id = `${ns}::${k}`;
                    if (!rowMap.has(id)) rowMap.set(id, { ns, key: k, cells: {} });
                }
            }
        }

        for (const row of rowMap.values()) {
            const srcPack = (srcMetaByNs[row.ns] || {})[row.key];
            row.keyContext = srcPack?.context != null ? String(srcPack.context) : '';
            for (const lang of state.allLangs) {
                const value = (trans[`${lang}|${row.ns}`] || {})[row.key];
                const m = lang === state.sourceLang ? null : (meta[`${lang}|${row.ns}`] || {})[row.key];
                row.cells[lang] = {
                    value: value ?? '',
                    confidence: m?.confidence ?? null,
                    translatedBy: m?.translated_by ?? '',
                    timestamp: m?.timestamp ?? '',
                    missing: value === undefined || value === null || value === '',
                    dirty: false,
                    saving: false,
                };
            }
        }

        state.rows = [...rowMap.values()];
        render();
    } catch (err) {
        console.error('Load error:', err);
        Utils.showToast('Error loading translations', 'error');
    }
}

// -----------------------------------------------------------------------
// Filtering
// -----------------------------------------------------------------------

// The target languages in view: every one, or the one the picker chose.
// Columns, counts, the Show filter and Fill all follow it.
function viewLangs() {
    return state.langView === 'all' ? state.targetLangs : [state.langView];
}

// Rows matching the search, before the Show filter, in namespace order.
function searchedRows() {
    const q = state.search.trim().toLowerCase();
    let rows = state.rows;
    if (q) {
        rows = rows.filter(r => {
            if (r.key.toLowerCase().includes(q)) return true;
            for (const l of state.allLangs) {
                if ((r.cells[l].value || '').toLowerCase().includes(q)) return true;
            }
            if ((r.keyContext || '').toLowerCase().includes(q)) return true;
            return false;
        });
    }
    return rows.slice().sort(compareKeys);
}

function visibleRows() {
    const rows = searchedRows();
    if (state.show === 'missing') return rows.filter(rowHasMissing);
    if (state.show === 'low') return rows.filter(rowHasIssues);
    return rows;
}

function compareKeys(a, b) {
    if (a.ns !== b.ns) return a.ns.localeCompare(b.ns);
    return a.key.localeCompare(b.key);
}

// A key with no source text is not a source key yet: its empty target cells
// can't be translated until the source is back-filled.
function isSourceKey(row) {
    return !row.cells[state.sourceLang].missing;
}
function isLowCell(cell, lang) {
    if (cell.missing || cell.confidence === null) return false;
    return cell.confidence < (state.confidenceByPair[lang] ?? 0.7);
}
function rowHasMissing(row) {
    return viewLangs().some(l => row.cells[l].missing);
}
function rowHasIssues(row) {
    return viewLangs().some(l => isLowCell(row.cells[l], l));
}

// -----------------------------------------------------------------------
// Rendering
// -----------------------------------------------------------------------
//
// The list is a flat run of items: a group header per namespace, then that
// namespace's keys when the group is open. While searching every group with
// a match is open, so a hit is never hidden behind a fold.

function isGroupOpen(ns) {
    return state.search.trim() !== '' || state.open.has(ns);
}

function buildItems(rows) {
    const items = [];
    let group = null;
    for (const row of rows) {
        if (!group || group.ns !== row.ns) {
            group = { type: 'group', ns: row.ns, rows: [] };
            items.push(group);
        }
        group.rows.push(row);
    }
    const out = [];
    for (const g of items) {
        out.push(g);
        if (isGroupOpen(g.ns)) for (const row of g.rows) out.push({ type: 'row', row });
    }
    return out;
}

// Virtual-list threshold. Below this many items, the list renders as a plain
// block flow. At or above it, windowed rendering (see renderVirtual) keeps
// ~2 screens of items in the DOM and absolute-positions them inside a sized
// container.
const VIRTUAL_THRESHOLD = 200;

// Overscan: extra items rendered above/below the viewport so scrolling feels
// smooth.
const VIRTUAL_OVERSCAN = 8;

// Fallback heights used until a real row and group header are measured.
let collapsedCardHeight = 34;
let groupHeight = 42;

// Vertical gap between rows. Matches the `.list` gap (2px) on #keysList in
// classic mode; virtual mode bakes it into each slot.
const VIRTUAL_CARD_GAP = 2;

const virtualState = {
    active: false,
    items: [],          // current items (group headers and key rows)
    heights: [],        // per-item measured height
    offsets: [],        // prefix sum of heights
    total: 0,           // sum of heights
    startIdx: -1,       // currently-rendered window [start, end)
    endIdx: -1,
    rafPending: false,
    scrollHandler: null,
    resizeHandler: null,
};

function render() {
    const list = document.getElementById('keysList');
    const empty = document.getElementById('keysEmpty');
    const rows = visibleRows();

    updateSummary(rows);
    renderColumns();

    if (rows.length === 0) {
        teardownVirtual();
        list.innerHTML = '';
        list.style.cssText = '';
        empty.textContent = emptyMessage();
        empty.classList.remove('hidden');
        return;
    }
    empty.classList.add('hidden');

    const items = buildItems(rows);
    list.style.setProperty('--lang-count', viewLangs().length);
    if (items.length < VIRTUAL_THRESHOLD) {
        teardownVirtual();
        list.removeAttribute('data-virtual');
        list.style.position = '';
        list.style.height = '';
        list.innerHTML = items.map(renderItem).join('');
        bindCardEvents();
        measureCollapsedHeight();
        return;
    }
    renderVirtual(items);
}

function emptyMessage() {
    if (state.search.trim()) return 'No keys match the search.';
    if (state.show === 'missing') return 'Nothing missing. Every key is translated.';
    if (state.show === 'low') return 'No low-confidence translations.';
    return 'No translations to show.';
}

// The language codes above the dot columns.
function renderColumns() {
    const cols = document.getElementById('keysCols');
    const langs = viewLangs();
    cols.style.setProperty('--lang-count', langs.length);
    cols.innerHTML = '<span></span><span></span><span></span><span></span>'
        + langs.map(l => `<span class="tx-col" title="${escapeAttr(Utils.langInfo(l).name)}">${escapeHtml(l)}</span>`).join('');
}

function renderItem(item) {
    return item.type === 'group' ? renderGroup(item) : renderCard(item.row);
}

// A namespace header: its name, size, low-confidence count, and per language
// how many of its source keys are translated. A phone shows the missing count
// instead, which fits a narrow column and is left out when nothing is missing.
function renderGroup(group) {
    const open = isGroupOpen(group.ns);
    const sourceRows = group.rows.filter(isSourceKey);
    const low = group.rows.filter(rowHasIssues).length;
    const cols = viewLangs().map(l => {
        const filled = sourceRows.filter(r => !r.cells[l].missing).length;
        const missing = sourceRows.length - filled;
        return `<span class="tx-group-count${missing ? '' : ' is-done'}" title="${filled} of ${sourceRows.length} ${escapeAttr(Utils.langInfo(l).name)} translated"><span class="tx-count-of">${filled}/${sourceRows.length}</span><span class="tx-count-missing">${missing || ''}</span></span>`;
    }).join('');
    const n = group.rows.length;
    return `
        <div class="tx-group" data-ns="${escapeAttr(group.ns)}" data-open="${open}" role="button" tabindex="0" aria-expanded="${open}">
            <span class="key-caret">${icon('chevron-right')}</span>
            <span class="tx-group-title">
                <span class="tx-group-name">${escapeHtml(group.ns)}</span>
                <span class="tx-group-meta">${n} key${n === 1 ? '' : 's'}</span>
                ${low ? `<span class="tx-group-low" title="${low} low confidence"><span class="tx-group-low-label">${low} low confidence</span></span>` : ''}
            </span>
            ${cols}
        </div>
    `;
}

// measureCollapsedHeight caches the height of a collapsed row and a group
// header so virtual mode has realistic estimates without laying out
// thousands of elements.
function measureCollapsedHeight() {
    const list = document.getElementById('keysList');
    const sample = list.querySelector('.key-card[data-expanded="false"]');
    if (sample) {
        const h = sample.getBoundingClientRect().height;
        if (h > 0) collapsedCardHeight = h + VIRTUAL_CARD_GAP;
    }
    const head = list.querySelector('.tx-group');
    if (head) {
        const h = head.getBoundingClientRect().height + parseFloat(getComputedStyle(head).marginTop || '0');
        if (h > 0) groupHeight = h + VIRTUAL_CARD_GAP;
    }
}

// ----- Virtual list ------------------------------------------------------
//
// Model: every item has an entry in `heights[]`. Group headers and collapsed
// rows use their measured heights; expanded rows use whatever we measured
// last time the row was in the window. `offsets[]` is the prefix sum so we
// can translate a scroll position into an item index with a binary search.

function itemHeight(item) {
    if (item.type === 'group') return groupHeight;
    const id = rowId(item.row);
    return state.expanded.has(id) ? (expandedHeightCache.get(id) || collapsedCardHeight * 3) : collapsedCardHeight;
}

function renderVirtual(items) {
    const list = document.getElementById('keysList');
    virtualState.active = true;
    virtualState.items = items;
    virtualState.heights = items.map(itemHeight);
    rebuildVirtualOffsets();
    virtualState.startIdx = -1;
    virtualState.endIdx = -1;

    list.setAttribute('data-virtual', 'true');
    list.style.position = 'relative';
    list.style.height = virtualState.total + 'px';

    attachVirtualScrollListeners();
    renderVirtualWindow();
}

// Height cache for expanded rows keyed by row id. Survives filter changes so
// re-opening a row doesn't flash a wrong estimate.
const expandedHeightCache = new Map();
function rowId(row) { return `${row.ns}::${row.key}`; }

function rebuildVirtualOffsets() {
    const n = virtualState.heights.length;
    virtualState.offsets = new Array(n);
    let acc = 0;
    for (let i = 0; i < n; i++) {
        virtualState.offsets[i] = acc;
        acc += virtualState.heights[i];
    }
    virtualState.total = acc;
}

// findItemAtOffset: binary search the prefix-sum offsets for the item whose
// slab contains y. Returns an index in [0, items.length].
function findItemAtOffset(y) {
    const o = virtualState.offsets;
    let lo = 0, hi = o.length - 1;
    while (lo < hi) {
        const mid = (lo + hi + 1) >>> 1;
        if (o[mid] <= y) lo = mid;
        else hi = mid - 1;
    }
    return lo;
}

function attachVirtualScrollListeners() {
    if (virtualState.scrollHandler) return;
    virtualState.scrollHandler = () => {
        if (virtualState.rafPending) return;
        virtualState.rafPending = true;
        requestAnimationFrame(() => {
            virtualState.rafPending = false;
            if (virtualState.active) renderVirtualWindow();
        });
    };
    virtualState.resizeHandler = virtualState.scrollHandler;
    window.addEventListener('scroll', virtualState.scrollHandler, { passive: true });
    window.addEventListener('resize', virtualState.resizeHandler);
}

function teardownVirtual() {
    if (!virtualState.active && !virtualState.scrollHandler) return;
    if (virtualState.scrollHandler) {
        window.removeEventListener('scroll', virtualState.scrollHandler);
        window.removeEventListener('resize', virtualState.resizeHandler);
    }
    virtualState.active = false;
    virtualState.scrollHandler = null;
    virtualState.resizeHandler = null;
    virtualState.startIdx = -1;
    virtualState.endIdx = -1;
}

// renderVirtualWindow computes the visible item range (with overscan) and
// re-renders #keysList only if that range moved. Items are positioned with
// `transform: translateY` so scrolling never touches the layout around them.
function renderVirtualWindow() {
    const list = document.getElementById('keysList');
    if (!virtualState.active || !list) return;

    // Don't shuffle the DOM while the user is typing in a cell: removing a
    // focused textarea would throw away their edit. The next scroll event
    // after they blur (or finish editing) will catch up.
    const active = document.activeElement;
    if (active && list.contains(active) && active.tagName === 'TEXTAREA') return;

    const listRect = list.getBoundingClientRect();
    const viewportTop = -listRect.top;
    const viewportBottom = viewportTop + window.innerHeight;

    const n = virtualState.items.length;
    let start = findItemAtOffset(Math.max(0, viewportTop));
    let end = findItemAtOffset(Math.max(0, viewportBottom)) + 1;
    start = Math.max(0, start - VIRTUAL_OVERSCAN);
    end = Math.min(n, end + VIRTUAL_OVERSCAN);

    if (start === virtualState.startIdx && end === virtualState.endIdx) return;
    virtualState.startIdx = start;
    virtualState.endIdx = end;

    const parts = new Array(end - start);
    for (let i = start; i < end; i++) {
        const top = virtualState.offsets[i];
        parts[i - start] =
            `<div class="v-card-wrap" data-vidx="${i}" style="transform:translateY(${top}px)">`
            + renderItem(virtualState.items[i])
            + `</div>`;
    }
    list.innerHTML = parts.join('');
    bindCardEvents();
    measureExpandedInWindow();
}

// measureExpandedInWindow re-measures the rendered wraps and reflows the
// list if any expanded row's real height differs from the estimate.
function measureExpandedInWindow() {
    const list = document.getElementById('keysList');
    let changed = false;
    list.querySelectorAll('.v-card-wrap').forEach(wrap => {
        const idx = +wrap.dataset.vidx;
        const card = wrap.querySelector('.key-card');
        if (!card) return;
        if (card.dataset.expanded !== 'true') return;
        const h = wrap.getBoundingClientRect().height + VIRTUAL_CARD_GAP;
        if (h > 0 && Math.abs(h - virtualState.heights[idx]) > 0.5) {
            virtualState.heights[idx] = h;
            expandedHeightCache.set(rowId(virtualState.items[idx].row), h);
            changed = true;
        }
    });
    if (!changed) return;

    rebuildVirtualOffsets();
    list.style.height = virtualState.total + 'px';
    list.querySelectorAll('.v-card-wrap').forEach(wrap => {
        const idx = +wrap.dataset.vidx;
        wrap.style.transform = `translateY(${virtualState.offsets[idx]}px)`;
    });
}

// One dot per language in view: done, missing, or below the pair's review
// threshold. The title says which, so no legend is needed.
function renderDot(row, lang) {
    const cell = row.cells[lang];
    const name = Utils.langInfo(lang).name;
    let stateName = 'done';
    let title = `${name}: translated`;
    if (cell.missing) {
        stateName = 'missing';
        title = `${name}: missing`;
    } else if (isLowCell(cell, lang)) {
        stateName = 'low';
        const thr = Math.round((state.confidenceByPair[lang] ?? 0.7) * 100);
        title = `${name}: ${Math.round(cell.confidence * 100)}% confidence, below the ${thr}% review threshold`;
    } else if (cell.confidence !== null && cell.translatedBy !== 'human' && cell.translatedBy !== 'human-edited') {
        title = `${name}: translated, ${Math.round(cell.confidence * 100)}% confidence`;
    }
    return `<span class="tx-dot" data-state="${stateName}" title="${escapeAttr(title)}"></span>`;
}

function renderCard(row) {
    const id = `${row.ns}::${row.key}`;
    const expanded = state.expanded.has(id);
    const source = row.cells[state.sourceLang];
    const sourceText = source.missing
        ? `<span class="key-source is-missing">No ${escapeHtml(Utils.langInfo(state.sourceLang).name)} text</span>`
        : `<span class="key-source">${escapeHtml(source.value)}</span>`;

    return `
        <div class="key-card" data-id="${escapeAttr(id)}" data-expanded="${expanded}">
            <div class="key-head" data-action="toggle">
                <span class="key-caret">${icon('chevron-right')}</span>
                <span class="key-name">${escapeHtml(row.key)}</span>
                ${sourceText}
                <span class="key-actions">
                    <button class="key-copy-btn ui-button ui-icon-button" data-action="copy-key"
                            title="Copy key to clipboard"
                            aria-label="Copy key ${escapeAttr(row.key)} to clipboard">${icon('copy')}</button>
                    <button class="key-delete-btn ui-button ui-icon-button" data-action="delete-key"
                            title="Delete this key (removes it from every language)"
                            aria-label="Delete key ${escapeAttr(row.key)}">${icon('trash')}</button>
                </span>
                ${viewLangs().map(l => renderDot(row, l)).join('')}
            </div>
            ${expanded ? `<div class="key-body">
                <div class="key-context-panel">
                    <label class="field-label">Translator context <span class="text-faint font-normal">(where this string appears; optional, included in LLM prompts)</span></label>
                    <textarea class="key-context-input ui-field input" rows="2" placeholder="e.g. Error banner on checkout; keep formal">${escapeHtml(row.keyContext || '')}</textarea>
                </div>
                ${renderLangRows(row)}
            </div>` : ''}
        </div>
    `;
}

function renderLangRows(row) {
    const langs = pickVisibleLangs();
    return langs.map(lang => renderLangRow(row, lang)).join('');
}

// An open key shows the source text above the languages in view, so the
// reader always has the base text next to what they're editing.
function pickVisibleLangs() {
    return [state.sourceLang, ...viewLangs()];
}

function renderLangRow(row, lang) {
    const cell = row.cells[lang] || { value: '', missing: true, confidence: null };
    const info = Utils.langInfo(lang);
    const isSource = lang === state.sourceLang;
    const threshold = state.confidenceByPair[lang] ?? 0.7;
    const isLow = !isSource && cell.confidence !== null && cell.confidence < threshold;

    const inputAttrs = [
        'class="lang-input ui-field"',
        `data-missing="${cell.missing}"`,
        `data-low="${isLow}"`,
        'rows="1"',
        `aria-label="${escapeAttr(info.name)}"`,
        cell.saving ? 'data-saving="true"' : '',
        cell.missing ? 'placeholder="Missing"' : '',
    ].filter(Boolean).join(' ');

    // The confidence pill sits beside every translated row so reviewers can
    // spot a low-confidence translation at a glance; who translated it and
    // when sit under the input.
    const chip = isSource
        ? '<span class="ui-status">source</span>'
        : renderConfidenceChip(cell, isSource, threshold);
    const details = !isSource ? renderDetails(cell) : '';

    // Source-row action:
    //   - If the source cell is missing AND at least one target cell has a
    //     value, offer a ✨ "back-fill from translation" button: the backend
    //     runs the LLM with (source=that target lang, target=sourceLang) using
    //     the existing target value as the prompt input. No-op if every
    //     target is also empty.
    //   - Otherwise: just copy.
    const copyBtn = `<button class="lang-icon-btn ui-button ui-icon-button" data-action="copy-source" title="Copy to clipboard" aria-label="Copy ${escapeAttr(info.name)} text">${icon('copy')}</button>`;
    let actions;
    if (isSource) {
        const backFillSource = cell.missing ? pickBackFillSource(row) : null;
        actions = backFillSource
            ? `
                <button class="lang-icon-btn ui-button ui-icon-button" data-action="fill-source"
                        data-from="${escapeAttr(backFillSource.lang)}"
                        title="Auto-fill ${escapeAttr(state.sourceLang)} from the existing ${escapeAttr(backFillSource.lang)} translation"
                        aria-label="Fill ${escapeAttr(state.sourceLang)} from ${escapeAttr(backFillSource.lang)}">
                    <span class="lang-icon-glyph">${icon('sparkles')}</span>
                </button>
                ${copyBtn}
            `
            : copyBtn;
    } else {
        actions = `
            <button class="lang-icon-btn ui-button ui-icon-button" data-action="translate" title="Auto-translate from ${escapeAttr(state.sourceLang)}" aria-label="Translate to ${escapeAttr(info.name)}">
                <span class="lang-icon-glyph">${icon('sparkles')}</span>
            </button>
            <button class="lang-icon-btn ui-button ui-icon-button" data-action="clear" title="Clear translation" aria-label="Clear ${escapeAttr(info.name)} translation">${icon('x')}</button>
        `;
    }

    return `
        <div class="lang-row" data-lang="${escapeAttr(lang)}">
            <div class="lang-label">
                <span class="lang-name">${escapeHtml(info.name)}</span>
                <span class="lang-code">${escapeHtml(lang)}</span>
            </div>
            <div class="lang-input-wrap">
                <textarea ${inputAttrs}>${escapeHtml(cell.value)}</textarea>
                ${details}
            </div>
            <div class="lang-pill">${chip}</div>
            <div class="lang-actions">${actions}</div>
        </div>
    `;
}

// renderConfidenceChip returns the always-on confidence pill for a
// non-source row. Tone is derived from the pair's threshold:
//   - `high`    (≥ threshold): ok
//   - `medium`  (≥ 0.5 but below threshold): warn
//   - `low`     (< 0.5): danger
//   - `human`   (edited by a person): info
//   - `unknown` (null / missing): no pill
function renderConfidenceChip(cell, isSource, threshold) {
    if (isSource || cell.missing || cell.confidence === null || cell.confidence === undefined) return '';
    const pct = Math.round(cell.confidence * 100);
    const thrPct = Math.round(threshold * 100);
    let tone = 'high';
    if (cell.confidence < 0.5) tone = 'low';
    else if (cell.confidence < threshold) tone = 'medium';
    const human = cell.translatedBy === 'human' || cell.translatedBy === 'human-edited';
    const label = human ? 'human' : `${pct}%`;

    // The chip's native tooltip is the first place people look. Make it
    // self-contained so someone hovering over "95%" immediately knows:
    // (1) it's the model's self-rating, not a correctness guarantee, and
    // (2) how it compares to this pair's review threshold.
    let title;
    if (human) {
        title = `Human-reviewed translation (${cell.translatedBy}) — no automatic scoring applied.`;
    } else {
        const toneWord = tone === 'high'
            ? `above the ${thrPct}% review threshold`
            : tone === 'medium'
                ? `below the ${thrPct}% threshold — flagged for review`
                : `well below the ${thrPct}% threshold — likely bad, review recommended`;
        title = `LLM self-reported confidence: ${pct}%. This is how sure the model itself is about its translation (not a quality guarantee). ${pct}% is ${toneWord}.`;
    }
    const toneClass = { high: 'ui-status-ok', medium: 'ui-status-warn', low: 'ui-status-danger', human: 'ui-status-info' };
    const t = human ? 'human' : tone;
    return `<span class="lang-confidence-chip ui-status ${toneClass[t]}" data-tone="${t}" title="${escapeAttr(title)}">${escapeHtml(label)}</span>`;
}

function renderDetails(cell) {
    const bits = [];
    if (cell.translatedBy) bits.push(`<span class="lang-detail-pill ui-chip">by ${escapeHtml(cell.translatedBy)}</span>`);
    if (cell.timestamp) {
        const dt = Utils.formatDate(cell.timestamp);
        if (dt !== 'N/A') bits.push(`<span class="lang-detail-pill ui-chip">${escapeHtml(dt)}</span>`);
    }
    return bits.length ? `<div class="lang-details">${bits.join('')}</div>` : '';
}

// Counts follow the Dashboard: only source keys are slots to translate;
// keys missing their source text are counted on their own. The summary sits
// in the top bar; the Show segments carry the key counts.
function updateSummary(rows) {
    const summary = document.getElementById('transSummary');
    const langs = viewLangs();
    let keys = 0;
    let filled = 0;
    let total = 0;
    for (const r of rows) {
        if (!isSourceKey(r)) continue;
        keys++;
        for (const l of langs) {
            total++;
            if (!r.cells[l].missing) filled++;
        }
    }
    const backFill = rows.length - keys;
    const back = backFill > 0 ? ` · ${backFill} key${backFill === 1 ? '' : 's'} missing ${Utils.langInfo(state.sourceLang).name}` : '';
    summary.textContent = `${filled} of ${total} translated${back}`;

    const searched = searchedRows();
    const counts = {
        all: searched.length,
        missing: searched.filter(rowHasMissing).length,
        low: searched.filter(rowHasIssues).length,
    };
    document.querySelectorAll('[data-count]').forEach(el => {
        el.textContent = counts[el.dataset.count];
    });
    updateFillButton(total - filled);
}

// The view's one primary action is filling what's missing. When nothing in
// the view is missing, the button steps aside and New key takes the lead.
function updateFillButton(missing) {
    const fill = document.getElementById('fillAllMissingBtn');
    const newKey = document.getElementById('newKeyBtn');
    if (fill.disabled) return; // a fill pass is running; it restores the label when done
    fill.textContent = `Fill ${missing} missing`;
    fill.hidden = missing === 0;
    newKey.classList.toggle('ui-button-primary', missing === 0);
}

// -----------------------------------------------------------------------
// Event binding for the rendered cards
// -----------------------------------------------------------------------

function bindCardEvents() {
    const list = document.getElementById('keysList');

    // toggleCard re-runs this after swapping one row, so every element is
    // bound once, marked, and skipped after.
    list.querySelectorAll('[data-action="toggle"]:not([data-bound])').forEach(el => {
        el.dataset.bound = 'true';
        el.addEventListener('click', (e) => {
            // Let clicks inside textareas/buttons pass through.
            if (e.target.closest('textarea, button')) return;
            const card = el.closest('.key-card');
            toggleCard(card.dataset.id);
        });
    });

    list.querySelectorAll('[data-action="delete-key"]:not([data-bound])').forEach(btn => {
        btn.dataset.bound = 'true';
        btn.addEventListener('click', (e) => {
            e.stopPropagation();
            const card = btn.closest('.key-card');
            if (!card) return;
            deleteKey(card.dataset.id, btn);
        });
    });

    list.querySelectorAll('[data-action="copy-key"]:not([data-bound])').forEach(btn => {
        btn.dataset.bound = 'true';
        btn.addEventListener('click', (e) => {
            e.stopPropagation();
            const card = btn.closest('.key-card');
            if (!card) return;
            copyKey(card.dataset.id, btn);
        });
    });

    list.querySelectorAll('.key-card[data-expanded="true"]:not([data-bound])').forEach(card => {
        card.dataset.bound = 'true';
        bindExpandedCard(card);
    });
}

async function deleteKey(id, btn) {
    const row = findRow(id);
    if (!row) return;
    const confirmMsg = `Delete "${row.key}" from namespace "${row.ns}"?\n\nThis will remove the key from every language and re-export each affected JSON file. This cannot be undone.`;
    if (!confirm(confirmMsg)) return;

    btn.disabled = true;
    const original = btn.innerHTML;
    btn.innerHTML = '<span class="spinner" aria-hidden="true"></span>';

    try {
        const res = await api.deleteKey(row.key, row.ns);
        if (!res.ok) {
            Utils.showToast(`Delete failed: ${res.error || 'unknown error'}`, 'error');
            btn.disabled = false;
            btn.innerHTML = original;
            return;
        }
        state.expanded.delete(id);
        state.rows = state.rows.filter(r => !(r.ns === row.ns && r.key === row.key));
        render();
        Utils.showToast(`Deleted ${row.ns}/${row.key}`, 'success');
    } catch (err) {
        console.error('deleteKey error', err);
        Utils.showToast(`Delete failed: ${err.message || err}`, 'error');
        btn.disabled = false;
        btn.innerHTML = original;
    }
}

// copyKey writes the key name to the clipboard so the user can paste it
// straight into application code (t('common.welcome'), …). We copy just the
// key — not `ns.key` — because the namespace is part of the row's metadata,
// not of the identifier most frameworks expect. Briefly flips the button to
// a check icon so the click produces visible feedback even when the toast
// is off-screen or dismissed too quickly.
async function copyKey(id, btn) {
    const row = findRow(id);
    if (!row) return;
    try {
        await writeToClipboard(row.key);
    } catch (err) {
        console.error('copyKey error', err);
        Utils.showToast(`Copy failed: ${err.message || err}`, 'error');
        return;
    }
    const original = btn.innerHTML;
    btn.innerHTML = icon('check');
    btn.setAttribute('data-copied', 'true');
    Utils.showToast(`Copied "${row.key}"`, 'success');
    setTimeout(() => {
        btn.innerHTML = original;
        btn.removeAttribute('data-copied');
    }, 1100);
}

// writeToClipboard prefers the async Clipboard API but falls back to the
// legacy `execCommand('copy')` trick whenever the Clipboard API is either
// unavailable (old browsers, non-secure origins) OR fails at runtime
// (e.g. DOMException: Document is not focused — common under automation
// and inside embedded WebViews). The legacy path works on localhost since
// the click counts as a user gesture.
async function writeToClipboard(text) {
    if (navigator.clipboard && window.isSecureContext) {
        try {
            await navigator.clipboard.writeText(text);
            return;
        } catch (err) {
            // Fall through to execCommand. We swallow the error here
            // because the legacy path can still succeed.
            console.debug('Clipboard API failed, falling back to execCommand:', err);
        }
    }
    const ta = document.createElement('textarea');
    ta.value = text;
    ta.setAttribute('readonly', '');
    ta.style.position = 'fixed';
    ta.style.top = '-1000px';
    document.body.appendChild(ta);
    ta.select();
    try {
        const ok = document.execCommand('copy');
        if (!ok) throw new Error('execCommand copy returned false');
    } finally {
        document.body.removeChild(ta);
    }
}

function bindExpandedCard(card) {
    const id = card.dataset.id;
    const row = findRow(id);
    if (!row) return;

    card.querySelectorAll('.lang-row').forEach(rowEl => {
        const lang = rowEl.dataset.lang;
        const input = rowEl.querySelector('textarea.lang-input');
        autoresize(input);

        input.addEventListener('input', () => {
            row.cells[lang].dirty = input.value !== row.cells[lang].value;
            input.dataset.dirty = row.cells[lang].dirty;
            autoresize(input);
        });
        input.addEventListener('keydown', (e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                input.blur();
            } else if (e.key === 'Escape') {
                e.preventDefault();
                input.value = row.cells[lang].value;
                row.cells[lang].dirty = false;
                input.dataset.dirty = 'false';
                input.blur();
            }
        });
        input.addEventListener('blur', () => {
            if (!row.cells[lang].dirty) return;
            const newValue = input.value;
            if (newValue === row.cells[lang].value) return;
            saveCell(row, lang, newValue);
        });

        rowEl.querySelectorAll('[data-action]').forEach(btn => {
            const action = btn.dataset.action;
            btn.addEventListener('click', (e) => {
                e.stopPropagation();
                if (action === 'translate') {
                    void translateCell(row, lang, btn).catch(err => {
                        console.error('translateCell:', err);
                        Utils.showToast(err?.message ? `Translation failed: ${err.message}` : 'Translation failed', 'error');
                    });
                } else if (action === 'clear') {
                    void clearCell(row, lang);
                } else if (action === 'copy-source') {
                    copyToClipboard(row.cells[state.sourceLang].value);
                } else if (action === 'fill-source') {
                    void fillSourceFromTarget(row, btn.dataset.from, btn).catch(err => {
                        console.error('fillSourceFromTarget:', err);
                        Utils.showToast(err?.message ? `Back-fill failed: ${err.message}` : 'Back-fill failed', 'error');
                    });
                }
            });
        });
    });

    const ctxTa = card.querySelector('.key-context-input');
    if (ctxTa) {
        autoresize(ctxTa);
        ctxTa.addEventListener('input', () => autoresize(ctxTa));
        ctxTa.addEventListener('keydown', (e) => {
            if (e.key === 'Escape') {
                e.preventDefault();
                ctxTa.value = row.keyContext;
                ctxTa.blur();
            }
        });
        ctxTa.addEventListener('blur', () => {
            if (ctxTa.value === row.keyContext) return;
            saveKeyContext(row, ctxTa);
        });
    }
}

function toggleCard(id) {
    if (state.expanded.has(id)) state.expanded.delete(id);
    else state.expanded.add(id);
    // Only re-render the affected card to keep the user's scroll position
    // stable and preserve focus state of any other expanded card's input.
    const card = document.querySelector(`.key-card[data-id="${cssEscape(id)}"]`);
    if (!card) { render(); return; }
    const row = findRow(id);
    if (!row) { render(); return; }
    const tmp = document.createElement('div');
    tmp.innerHTML = renderCard(row);
    card.replaceWith(tmp.firstElementChild);
    bindCardEvents();

    // In virtual mode, expanding/collapsing a card changes its height, which
    // shifts every subsequent card's offset. measureExpandedInWindow handles
    // the expand path; for collapse we reset the entry to the baseline and
    // drop the cached height so a later re-expand starts clean.
    if (virtualState.active) {
        const idx = virtualState.items.findIndex(it => it.type === 'row' && rowId(it.row) === id);
        if (idx >= 0) {
            if (state.expanded.has(id)) {
                measureExpandedInWindow();
            } else {
                virtualState.heights[idx] = collapsedCardHeight;
                expandedHeightCache.delete(id);
                rebuildVirtualOffsets();
                const list = document.getElementById('keysList');
                list.style.height = virtualState.total + 'px';
                list.querySelectorAll('.v-card-wrap').forEach(wrap => {
                    const i = +wrap.dataset.vidx;
                    wrap.style.transform = `translateY(${virtualState.offsets[i]}px)`;
                });
            }
        }
    }

    // Focus the first non-source input when expanding, so editing a missing
    // translation is one click away.
    if (state.expanded.has(id)) {
        const fresh = document.querySelector(`.key-card[data-id="${cssEscape(id)}"]`);
        const firstTarget = fresh?.querySelector(`.lang-row:not([data-lang="${cssEscape(state.sourceLang)}"]) textarea`);
        firstTarget?.focus();
    }
}

// -----------------------------------------------------------------------
// Mutations
// -----------------------------------------------------------------------

/** Persists reviewer "translator context" on the source language metadata row (LLM reads it when translating to any target). */
async function saveKeyContext(row, ta) {
    const newCtx = ta.value;
    if (newCtx === row.keyContext) return;

    const metaRes = await api.getMetadata(state.sourceLang, row.ns);
    const pack = metaRes.ok ? (metaRes.data?.metadata || {})[row.key] : {};
    const payload = {
        confidence: pack?.confidence ?? 0,
        translated_by: pack?.translated_by ?? '',
        timestamp: pack?.timestamp ?? '',
        context: newCtx,
    };

    const r = await api.updateMetadata(state.sourceLang, row.key, payload, row.ns);
    if (!r.ok) {
        Utils.showToast(`Could not save context: ${r.error}`, 'error');
        ta.value = row.keyContext;
        return;
    }
    row.keyContext = newCtx;
    Utils.showToast('Translator context saved', 'success');
}

async function saveCell(row, lang, newValue) {
    const cell = row.cells[lang];
    cell.saving = true;
    reflectCell(row, lang);
    try {
        const result = await api.updateTranslation(lang, row.key, newValue, null, row.ns);
        if (!result.ok) {
            Utils.showToast(`Save failed: ${result.error}`, 'error');
            cell.saving = false;
            reflectCell(row, lang);
            return;
        }
        cell.value = newValue;
        cell.missing = newValue === '';
        cell.dirty = false;
        cell.confidence = 1;
        cell.translatedBy = 'human';
        cell.timestamp = new Date().toISOString();
        cell.saving = false;
        reflectCell(row, lang);
        rerenderCardHead(row);
    } catch (err) {
        console.error('saveCell:', err);
        cell.saving = false;
        reflectCell(row, lang);
        Utils.showToast('Error saving translation', 'error');
    }
}

// pickBackFillSource picks the "best" target cell to back-translate into the
// source language. Strategy: prefer a human-edited cell (most trustworthy),
// then highest-confidence MT cell, then most recent, then any non-empty.
// Returns null if every target cell is empty — the ✨ button is hidden in
// that case.
function pickBackFillSource(row) {
    const candidates = [];
    for (const lang of state.targetLangs) {
        const c = row.cells[lang];
        if (!c || !c.value || !c.value.trim()) continue;
        candidates.push({ lang, cell: c });
    }
    if (candidates.length === 0) return null;

    const isHuman = c => c.translatedBy === 'human' || c.translatedBy === 'human-edited';
    candidates.sort((a, b) => {
        const ah = isHuman(a.cell) ? 1 : 0;
        const bh = isHuman(b.cell) ? 1 : 0;
        if (ah !== bh) return bh - ah;
        const ac = a.cell.confidence ?? -1;
        const bc = b.cell.confidence ?? -1;
        if (ac !== bc) return bc - ac;
        const at = a.cell.timestamp ? Date.parse(a.cell.timestamp) : 0;
        const bt = b.cell.timestamp ? Date.parse(b.cell.timestamp) : 0;
        return bt - at;
    });
    return candidates[0];
}

// fillSourceFromTarget back-translates the chosen target cell into the source
// language and writes the result into the source cell. The backend endpoint
// honours our `source_text` override, so nothing has to exist in the DB for
// the source key — that's the whole point of this flow.
async function fillSourceFromTarget(row, fromLang, btn) {
    const fromCell = row.cells[fromLang];
    if (!fromCell || !fromCell.value) {
        Utils.showToast(`No ${fromLang} text to back-translate from`, 'error');
        return;
    }
    Utils.showToast('Filling base language from translation…', 'info', 5000);
    const sourceLang = state.sourceLang;
    const sourceCell = row.cells[sourceLang];
    if (!sourceCell) {
        Utils.showToast('Missing source language cell — try refreshing the page', 'error');
        return;
    }
    sourceCell.saving = true;
    btn.dataset.busy = 'true';
    reflectCell(row, sourceLang);

    try {
        // Reversed direction: translate (fromLang -> sourceLang) using the
        // existing target value as the input text.
        const r = await api.translateSingle(
            row.key, sourceLang, fromLang, row.ns,
            /*dryRun=*/false, fromCell.value,
        );
        if (!r.ok) {
            Utils.showToast(`Back-fill failed: ${r.error}`, 'error');
            sourceCell.saving = false;
            btn.dataset.busy = 'false';
            reflectCell(row, sourceLang);
            return;
        }
        const data = r.data || {};
        sourceCell.value = data.translation ?? '';
        // The source cell has no confidence chip (it's the reference) but we
        // still store the metadata so the details strip can show provenance.
        sourceCell.confidence = data.confidence ?? null;
        sourceCell.translatedBy = data.model_used ?? '';
        sourceCell.timestamp = new Date().toISOString();
        sourceCell.missing = !sourceCell.value;
        sourceCell.dirty = false;
        sourceCell.saving = false;
        btn.dataset.busy = 'false';
        reflectCell(row, sourceLang);
        rerenderCardHead(row);
        Utils.showToast(`Filled ${sourceLang} from ${fromLang}`, 'success');
    } catch (err) {
        console.error('fillSourceFromTarget:', err);
        sourceCell.saving = false;
        btn.dataset.busy = 'false';
        reflectCell(row, sourceLang);
        Utils.showToast('Back-fill error', 'error');
    }
}

async function translateCell(row, lang, btn) {
    const srcCell = row.cells[state.sourceLang];
    if (!srcCell) {
        Utils.showToast('Missing source language cell — try refreshing the page', 'error');
        return;
    }
    const sourceText = srcCell.value;
    if (!sourceText || !String(sourceText).trim()) {
        Utils.showToast('No source text to translate from', 'error');
        return;
    }
    const cell = row.cells[lang];
    if (!cell) {
        Utils.showToast(`No cell for language “${lang}” — try refreshing the page`, 'error');
        return;
    }

    cell.saving = true;
    btn.dataset.busy = 'true';
    reflectCell(row, lang);
    Utils.showToast('Translating with the LLM… (can take 1–2 minutes)', 'info', 10000);

    try {
        const r = await api.translateSingle(row.key, lang, state.sourceLang, row.ns);
        if (!r.ok) {
            Utils.showToast(`Translation failed: ${r.error}`, 'error');
            return;
        }
        const data = r.data || {};
        cell.value = data.translation ?? '';
        cell.confidence = data.confidence ?? null;
        cell.translatedBy = data.model_used ?? '';
        cell.timestamp = new Date().toISOString();
        cell.missing = !cell.value;
        cell.dirty = false;
        reflectCell(row, lang);
        rerenderCardHead(row);
        Utils.showToast(`Updated ${lang}`, 'success', 2500);

        // Glossary enforcement is handled server-side: the LLM client
        // auto-retries with a correction prompt when it detects violations,
        // so 99% of the time `data.glossary_violations` is empty here. The
        // remaining 1% (retries exhausted, still wrong) is signalled by the
        // warn confidence pill — no toast needed, the pill already tells
        // the story. We deliberately drop the old "Glossary not respected"
        // toast because users complained it was noise, not signal.
    } catch (err) {
        console.error('translateCell:', err);
        Utils.showToast(err?.message ? `Translation error: ${err.message}` : 'Translation error', 'error');
    } finally {
        cell.saving = false;
        btn.dataset.busy = 'false';
        reflectCell(row, lang);
    }
}

async function clearCell(row, lang) {
    if (!confirm(`Clear ${lang} translation for "${row.key}"?`)) return;
    await saveCell(row, lang, '');
}

// reflectCell writes the latest cell state onto the DOM without rebuilding
// the whole row — keeps focus/selection intact while the user is editing.
function reflectCell(row, lang) {
    const card = document.querySelector(`.key-card[data-id="${cssEscape(`${row.ns}::${row.key}`)}"]`);
    if (!card) return;
    const rowEl = card.querySelector(`.lang-row[data-lang="${cssEscape(lang)}"]`);
    if (!rowEl) return;
    const input = rowEl.querySelector('textarea.lang-input');
    const cell = row.cells[lang];
    if (input && document.activeElement !== input) {
        input.value = cell.value;
        autoresize(input);
    }
    input.dataset.missing = cell.missing;
    const threshold = state.confidenceByPair[lang] ?? 0.7;
    const isLow = lang !== state.sourceLang && cell.confidence !== null && cell.confidence < threshold;
    input.dataset.low = isLow;
    input.dataset.dirty = cell.dirty;
    if (cell.saving) input.dataset.saving = 'true';
    else delete input.dataset.saving;

    // Refresh the always-visible confidence pill so a newly-translated cell
    // shows its score without needing a full re-render. The source row keeps
    // its "source" pill.
    const pill = rowEl.querySelector('.lang-pill');
    if (pill && lang !== state.sourceLang) {
        pill.innerHTML = renderConfidenceChip(cell, false, threshold);
    }

    // Rebuild the translator and date strip in place.
    if (lang !== state.sourceLang) {
        const wrap = rowEl.querySelector('.lang-input-wrap');
        const existing = wrap.querySelector('.lang-details');
        if (existing) existing.remove();
        const html = renderDetails(cell);
        if (html) wrap.insertAdjacentHTML('beforeend', html);
    }
}

function rerenderCardHead(row) {
    const id = `${row.ns}::${row.key}`;
    const card = document.querySelector(`.key-card[data-id="${cssEscape(id)}"]`);
    if (!card) return;
    const head = card.querySelector('.key-head');
    if (!head) return;
    const tmp = document.createElement('div');
    tmp.innerHTML = renderCard(row);
    const newCard = tmp.firstElementChild;
    head.replaceWith(newCard.querySelector('.key-head'));
    // Re-bind the toggle for the refreshed head.
    const newHead = card.querySelector('.key-head');
    newHead.addEventListener('click', (e) => {
        if (e.target.closest('textarea, button')) return;
        toggleCard(id);
    });
    newHead.querySelector('[data-action="copy-key"]')?.addEventListener('click', (e) => {
        e.stopPropagation();
        copyKey(id, e.currentTarget);
    });
    newHead.querySelector('[data-action="delete-key"]')?.addEventListener('click', (e) => {
        e.stopPropagation();
        deleteKey(id, e.currentTarget);
    });

    // The row stays where it is, even if it no longer matches the Show
    // filter; its group's counts and the summary catch up now.
    const groupEl = document.querySelector(`.tx-group[data-ns="${cssEscape(row.ns)}"]`);
    if (groupEl) {
        const rows = visibleRows().filter(r => r.ns === row.ns);
        const tmpGroup = document.createElement('div');
        tmpGroup.innerHTML = renderGroup({ type: 'group', ns: row.ns, rows: rows.length ? rows : [row] });
        groupEl.replaceWith(tmpGroup.firstElementChild);
    }
    updateSummary(visibleRows());
}

function toggleGroup(ns) {
    if (state.search.trim()) return; // every group with a hit stays open while searching
    if (state.open.has(ns)) state.open.delete(ns);
    else state.open.add(ns);
    saveOpenGroups();
    render();
}

// -----------------------------------------------------------------------
// Toolbar
// -----------------------------------------------------------------------

function setupToolbar() {
    document.getElementById('searchInput').addEventListener('input', Utils.debounce((e) => {
        state.search = e.target.value;
        render();
    }, 150));

    document.getElementById('fillAllMissingBtn').addEventListener('click', fillAllMissing);
    document.getElementById('newKeyBtn').addEventListener('click', () => openNewKeyModal({ mode: 'key' }));

    document.querySelectorAll('[data-show]').forEach(btn => {
        btn.addEventListener('click', () => {
            state.show = btn.dataset.show;
            document.querySelectorAll('[data-show]').forEach(b => {
                b.setAttribute('aria-pressed', String(b.dataset.show === state.show));
            });
            render();
        });
    });

    // Group headers are redrawn often; one listener on the list serves them.
    const list = document.getElementById('keysList');
    list.addEventListener('click', (e) => {
        const head = e.target.closest('.tx-group');
        if (head) toggleGroup(head.dataset.ns);
    });
    list.addEventListener('keydown', (e) => {
        const head = e.target.closest('.tx-group');
        if (!head || (e.key !== 'Enter' && e.key !== ' ')) return;
        e.preventDefault();
        toggleGroup(head.dataset.ns);
    });

    document.querySelectorAll('[data-menu-toggle]').forEach(btn => {
        btn.addEventListener('click', (e) => {
            e.stopPropagation();
            toggleMenu(btn.dataset.menuToggle, btn);
        });
    });

    document.querySelector('[data-ns-action="new"]').addEventListener('click', () => {
        closeAllMenus();
        openNewKeyModal({ mode: 'namespace' });
    });
    document.querySelector('[data-ns-action="manage"]').addEventListener('click', () => {
        closeAllMenus();
        openNsManageModal();
    });

    document.addEventListener('click', (e) => {
        if (!e.target.closest('.tx-menu')) closeAllMenus();
    });
}

// Coming back to the window reloads quietly, so edits made elsewhere (the
// JSON files, another tab) show up without a refresh button. Never while a
// cell is being edited or a fill pass runs.
function setupAutoReload() {
    const maybeReload = () => {
        if (document.visibilityState !== 'visible') return;
        if (Date.now() - state.lastLoad < 5000) return;
        if (document.getElementById('fillAllMissingBtn').disabled) return;
        const active = document.activeElement;
        if (active && active.closest?.('#keysList') && active.tagName === 'TEXTAREA') return;
        if (state.rows.some(r => Object.values(r.cells).some(c => c.dirty || c.saving))) return;
        void loadAll({ quiet: true });
    };
    document.addEventListener('visibilitychange', maybeReload);
    window.addEventListener('focus', maybeReload);
}

function toggleMenu(key, btn) {
    const panel = document.getElementById(`${key}MenuPanel`);
    if (!panel) return;
    const open = !panel.classList.contains('hidden');
    closeAllMenus();
    if (!open) {
        panel.classList.remove('hidden');
        btn.setAttribute('aria-expanded', 'true');
    }
}
function closeAllMenus() {
    document.querySelectorAll('.tx-menu-panel').forEach(p => p.classList.add('hidden'));
    document.querySelectorAll('[data-menu-toggle]').forEach(b => b.setAttribute('aria-expanded', 'false'));
}

// The language picker: every target language, or one. The source language
// always shows in an open key.
function renderLangsMenu() {
    const panel = document.getElementById('langsMenuPanel');
    const items = [{ value: 'all', label: 'All languages', icon: 'globe' }];
    for (const lang of state.targetLangs) {
        items.push({ value: lang, label: Utils.langInfo(lang).name, code: lang });
    }
    panel.innerHTML = items.map(item => `
        <button class="tx-menu-item ui-option" role="menuitemradio" data-lang-view="${escapeAttr(item.value)}" aria-selected="${state.langView === item.value}">
            ${item.icon ? icon(item.icon) : ''}
            <span>${escapeHtml(item.label)}</span>
            ${item.code ? `<span class="tx-menu-code">${escapeHtml(item.code)}</span>` : ''}
        </button>
    `).join('');
    panel.querySelectorAll('[data-lang-view]').forEach(btn => {
        btn.addEventListener('click', () => {
            state.langView = btn.dataset.langView;
            renderLangsMenu();
            updateToolbarLabels();
            closeAllMenus();
            render();
        });
    });
}

function updateToolbarLabels() {
    const langsLabel = document.getElementById('langsBtnLabel');
    langsLabel.textContent = state.langView === 'all'
        ? (state.targetLangs.join(' · ') || 'All languages')
        : Utils.langInfo(state.langView).name;
}

// -----------------------------------------------------------------------
// Bulk fill
// -----------------------------------------------------------------------

async function fillAllMissing() {
    const rows = visibleRows().filter(r => isSourceKey(r) && rowHasMissing(r));
    if (rows.length === 0) {
        Utils.showToast('Nothing missing in the current view', 'info');
        return;
    }
    if (!confirm(`Auto-translate missing cells for ${rows.length} key${rows.length === 1 ? '' : 's'}?`)) return;

    // Group by (target_lang, namespace) and use the batch endpoint — much
    // faster than N /translate/single calls.
    const groups = new Map();
    for (const r of rows) {
        for (const lang of viewLangs()) {
            if (!r.cells[lang].missing) continue;
            const k = `${lang}|${r.ns}`;
            if (!groups.has(k)) groups.set(k, { lang, ns: r.ns });
        }
    }

    const btn = document.getElementById('fillAllMissingBtn');
    btn.disabled = true;
    btn.textContent = `Filling ${groups.size} group${groups.size === 1 ? '' : 's'}…`;

    try {
        for (const { lang, ns } of groups.values()) {
            const res = await api.translateMissing([lang], state.sourceLang, ns);
            if (!res.ok) {
                Utils.showToast(`Batch failed ${lang}/${ns}: ${res.error}`, 'error');
            }
        }
        Utils.showToast('Translation pass complete', 'success');
        await loadAll();
    } finally {
        btn.disabled = false;
        updateSummary(visibleRows());
    }
}

// -----------------------------------------------------------------------
// Utilities
// -----------------------------------------------------------------------

function findRow(id) {
    return state.rows.find(r => `${r.ns}::${r.key}` === id) || null;
}

function autoresize(ta) {
    if (!ta) return;
    ta.style.height = 'auto';
    ta.style.height = `${Math.max(ta.scrollHeight, 32)}px`;
}

function copyToClipboard(s) {
    if (!s) return;
    navigator.clipboard.writeText(s).then(
        () => Utils.showToast('Copied', 'success'),
        () => Utils.showToast('Copy failed', 'error')
    );
}

function setupKeyboardShortcuts() {
    document.addEventListener('keydown', (e) => {
        const isMeta = e.metaKey || e.ctrlKey;
        if (isMeta && e.key === '1') { e.preventDefault(); window.location.href = '/'; return; }
        if (isMeta && e.key === '2') { e.preventDefault(); window.location.href = '/ui/translations.html'; return; }
        if (isMeta && e.key === '3') { e.preventDefault(); window.location.href = '/ui/missing.html'; return; }
        if (isMeta && e.key === '4') { e.preventDefault(); window.location.href = '/ui/glossary.html'; return; }
        if (isMeta && e.key === '5') { e.preventDefault(); window.location.href = '/ui/settings.html'; return; }
        if (isMeta && e.key === '6') { e.preventDefault(); window.location.href = '/ui/import.html'; return; }
        if (isMeta && e.key === 'f') {
            e.preventDefault();
            document.getElementById('searchInput').focus();
            return;
        }
        if (isMeta && (e.key === 'n' || e.key === 'N')) {
            e.preventDefault();
            openNewKeyModal();
            return;
        }
        if (e.key === 'Escape') {
            if (!document.getElementById('nsManageModal').classList.contains('hidden')) {
                closeNsManageModal();
            } else if (!document.getElementById('newKeyModal').classList.contains('hidden')) {
                closeNewKeyModal();
            } else {
                closeAllMenus();
            }
        }
    });
}

// -----------------------------------------------------------------------
// New-key modal
// -----------------------------------------------------------------------

const NEW_NS_SENTINEL = '__new__';

function setupNewKeyModal() {
    document.getElementById('newKeyCloseBtn').addEventListener('click', closeNewKeyModal);
    document.getElementById('newKeyCancelBtn').addEventListener('click', closeNewKeyModal);
    document.getElementById('newKeySubmitBtn').addEventListener('click', submitNewKey);

    document.getElementById('newKeyModal').addEventListener('click', (e) => {
        // Click on backdrop (outside dialog panel) closes.
        if (e.target.id === 'newKeyModal') closeNewKeyModal();
    });

    document.getElementById('newKeyNamespace').addEventListener('change', (e) => {
        const newInput = document.getElementById('newKeyNamespaceNew');
        if (e.target.value === NEW_NS_SENTINEL) {
            newInput.classList.remove('hidden');
            newInput.focus();
        } else {
            newInput.classList.add('hidden');
        }
    });

    // Enter in key/value fields → submit (plain Enter in textarea still newline).
    document.getElementById('newKeyName').addEventListener('keydown', (e) => {
        if (e.key === 'Enter') { e.preventDefault(); submitNewKey(); }
    });
    document.getElementById('newKeyNamespaceNew').addEventListener('keydown', (e) => {
        if (e.key === 'Enter') { e.preventDefault(); document.getElementById('newKeyName').focus(); }
    });
}

// openNewKeyModal displays the "Add translation key" modal. Pass
// `{ mode: 'namespace' }` to reskin it as "Create namespace" — the NS dropdown
// starts pre-set to "+ New namespace…" with the NS input focused. Either mode
// writes the same payload under the hood (one PUT of the source-language
// value), because the data model keys everything on translations so a
// namespace can't exist without at least one key.
function openNewKeyModal({ mode = 'key' } = {}) {
    const sel = document.getElementById('newKeyNamespace');
    const existing = state.namespaces.length ? state.namespaces : ['translation'];
    const opts = existing.map(ns => `<option value="${escapeAttr(ns)}">${escapeHtml(ns)}</option>`).join('');
    sel.innerHTML = opts + `<option value="${NEW_NS_SENTINEL}">+ New namespace…</option>`;

    const newInput = document.getElementById('newKeyNamespaceNew');
    newInput.value = '';
    newInput.classList.add('hidden');

    document.getElementById('newKeyName').value = '';
    document.getElementById('newKeySourceValue').value = '';
    document.getElementById('newKeySourceLangLabel').textContent = state.sourceLang;
    setNewKeyError('');

    const titleEl   = document.getElementById('newKeyModalTitle');
    const helpEl    = document.getElementById('newKeyModalHelp');
    const submitBtn = document.getElementById('newKeySubmitBtn');

    if (mode === 'namespace') {
        titleEl.textContent = 'Create namespace';
        helpEl.innerHTML = 'A namespace needs at least one key to be saved. Name the namespace below, then add your first key and its source text.';
        submitBtn.textContent = 'Create namespace';
        sel.value = NEW_NS_SENTINEL;
        newInput.classList.remove('hidden');
    } else {
        titleEl.textContent = 'Add translation key';
        helpEl.innerHTML = 'Create a new translation key in an existing namespace, or pick "+ New namespace…" to start a new one.';
        submitBtn.textContent = 'Create key';
        // Default to the first open group: that's where the reader is working.
        sel.value = [...state.open].find(ns => existing.includes(ns)) || existing[0];
    }

    document.getElementById('newKeyModal').classList.remove('hidden');
    setTimeout(() => {
        if (mode === 'namespace') newInput.focus();
        else document.getElementById('newKeyName').focus();
    }, 0);
}

function closeNewKeyModal() {
    document.getElementById('newKeyModal').classList.add('hidden');
    setNewKeyError('');
    setNewKeyBusy(false);
}

function setNewKeyError(msg) {
    const el = document.getElementById('newKeyError');
    if (!msg) {
        el.classList.add('hidden');
        el.textContent = '';
    } else {
        el.classList.remove('hidden');
        el.textContent = msg;
    }
}

function setNewKeyBusy(busy) {
    const btn = document.getElementById('newKeySubmitBtn');
    btn.disabled = !!busy;
    if (busy) {
        btn.dataset.prevLabel = btn.textContent;
        btn.textContent = 'Creating…';
    } else if (btn.dataset.prevLabel) {
        btn.textContent = btn.dataset.prevLabel;
        delete btn.dataset.prevLabel;
    }
}

async function submitNewKey() {
    const sel = document.getElementById('newKeyNamespace');
    const rawNs = sel.value === NEW_NS_SENTINEL
        ? document.getElementById('newKeyNamespaceNew').value
        : sel.value;
    const ns = (rawNs || '').trim();
    const key = document.getElementById('newKeyName').value.trim();
    const sourceValue = document.getElementById('newKeySourceValue').value;

    if (!ns) { setNewKeyError('Namespace is required.'); return; }
    if (!/^[A-Za-z0-9._-]+$/.test(ns)) {
        setNewKeyError('Namespace may only contain letters, digits, dot, dash, underscore.');
        return;
    }
    if (!key) { setNewKeyError('Key is required.'); return; }
    if (/\s/.test(key)) { setNewKeyError('Key cannot contain whitespace.'); return; }

    const existing = state.rows.find(r => r.ns === ns && r.key === key);
    if (existing) {
        setNewKeyError(`Key "${key}" already exists in "${ns}". Close this dialog and find it in the list.`);
        return;
    }

    setNewKeyError('');
    setNewKeyBusy(true);
    try {
        const result = await api.updateTranslation(state.sourceLang, key, sourceValue, null, ns);
        if (!result.ok) {
            setNewKeyError(`Create failed: ${result.error || 'unknown error'}`);
            setNewKeyBusy(false);
            return;
        }

        if (!state.namespaces.includes(ns)) state.namespaces.push(ns);
        state.expanded.add(`${ns}::${key}`);

        state.open.add(ns);
        saveOpenGroups();
        closeNewKeyModal();
        await loadAll();

        Utils.showToast(`Created ${ns}/${key}`, 'success');

        // Scroll the brand-new card into view so the user sees it right away.
        const card = document.querySelector(`.key-card[data-id="${cssEscape(`${ns}::${key}`)}"]`);
        if (card) card.scrollIntoView({ behavior: 'smooth', block: 'center' });
    } catch (err) {
        console.error('submitNewKey:', err);
        setNewKeyError('Unexpected error. See console.');
        setNewKeyBusy(false);
    }
}

// -----------------------------------------------------------------------
// Manage-namespaces modal
//
// Opened from the namespace dropdown. Shows every known namespace with its
// key/translation counts and lets users rename or delete it. All work goes
// through the `/api/namespaces/:ns` PUT/DELETE endpoints; on success we
// reload the translations so `state.namespaces` + `state.rows` reflect the
// new layout.
// -----------------------------------------------------------------------

// nsManageEntries caches the last fetched list so the inline rename + delete
// flows can tell if a change actually affected something before re-rendering.
let nsManageEntries = [];

function setupNsManageModal() {
    document.getElementById('nsManageCloseBtn').addEventListener('click', closeNsManageModal);
    document.getElementById('nsManageDoneBtn').addEventListener('click', closeNsManageModal);
    document.getElementById('nsManageModal').addEventListener('click', (e) => {
        if (e.target.id === 'nsManageModal') closeNsManageModal();
    });
}

async function openNsManageModal() {
    setNsManageError('');
    document.getElementById('nsManageModal').classList.remove('hidden');
    await refreshNsManageList();
}

function closeNsManageModal() {
    document.getElementById('nsManageModal').classList.add('hidden');
    setNsManageError('');
}

function setNsManageError(msg) {
    const el = document.getElementById('nsManageError');
    if (!msg) {
        el.classList.add('hidden');
        el.textContent = '';
    } else {
        el.classList.remove('hidden');
        el.textContent = msg;
    }
}

async function refreshNsManageList() {
    const list = document.getElementById('nsManageList');
    const empty = document.getElementById('nsManageEmpty');
    Utils.showLoading(list);
    empty.classList.add('hidden');

    const resp = await api.getNamespaceStats();
    if (!resp.ok) {
        console.error('getNamespaceStats:', resp.error);
        setNsManageError(`Failed to load namespaces: ${resp.error}`);
        list.innerHTML = '';
        return;
    }
    nsManageEntries = Array.isArray(resp.data?.namespaces) ? resp.data.namespaces : [];

    if (!nsManageEntries.length) {
        list.innerHTML = '';
        empty.classList.remove('hidden');
        return;
    }
    renderNsManageList();
}

function renderNsManageList() {
    const list = document.getElementById('nsManageList');
    list.innerHTML = nsManageEntries.map(entry => `
        <div class="ns-manage-row ui-row" data-ns="${escapeAttr(entry.namespace)}">
            <span class="text-muted">${icon('folder')}</span>
            <div class="flex-1 min-w-0">
                <div class="ns-manage-name" data-role="display">${escapeHtml(entry.namespace)}</div>
                <input type="text" value="${escapeAttr(entry.namespace)}" data-role="edit" aria-label="New name for ${escapeAttr(entry.namespace)}"
                       class="hidden ui-field input w-full font-mono text-code">
                <div class="ns-manage-counts">
                    ${entry.key_count} key${entry.key_count === 1 ? '' : 's'}
                    · ${entry.translation_count} translation${entry.translation_count === 1 ? '' : 's'}
                </div>
            </div>
            <div class="flex items-center gap-1" data-role="actions-view">
                <button class="ui-button" data-action="rename">Rename</button>
                <button class="ui-button ui-button-danger" data-action="delete">Delete</button>
            </div>
            <div class="hidden items-center gap-1" data-role="actions-edit">
                <button class="ui-button" data-action="rename-save">Save</button>
                <button class="ui-button" data-action="rename-cancel">Cancel</button>
            </div>
        </div>
    `).join('');

    list.querySelectorAll('.ns-manage-row').forEach(row => {
        const ns = row.dataset.ns;
        row.querySelector('[data-action="rename"]').addEventListener('click', () => enterRenameMode(row));
        row.querySelector('[data-action="rename-cancel"]').addEventListener('click', () => exitRenameMode(row));
        row.querySelector('[data-action="rename-save"]').addEventListener('click', () => saveRename(row, ns));
        row.querySelector('[data-action="delete"]').addEventListener('click', () => deleteNsFromManage(ns));
        row.querySelector('[data-role="edit"]').addEventListener('keydown', (e) => {
            if (e.key === 'Enter') { e.preventDefault(); saveRename(row, ns); }
            if (e.key === 'Escape') { e.preventDefault(); exitRenameMode(row); }
        });
    });
}

function enterRenameMode(row) {
    row.querySelector('[data-role="display"]').classList.add('hidden');
    row.querySelector('[data-role="actions-view"]').classList.add('hidden');
    const edit = row.querySelector('[data-role="edit"]');
    edit.classList.remove('hidden');
    row.querySelector('[data-role="actions-edit"]').classList.remove('hidden');
    row.querySelector('[data-role="actions-edit"]').classList.add('flex');
    setTimeout(() => { edit.focus(); edit.select(); }, 0);
}

function exitRenameMode(row) {
    row.querySelector('[data-role="display"]').classList.remove('hidden');
    row.querySelector('[data-role="actions-view"]').classList.remove('hidden');
    const edit = row.querySelector('[data-role="edit"]');
    edit.classList.add('hidden');
    edit.value = row.dataset.ns;
    const actions = row.querySelector('[data-role="actions-edit"]');
    actions.classList.add('hidden');
    actions.classList.remove('flex');
}

async function saveRename(row, oldNs) {
    const newNs = row.querySelector('[data-role="edit"]').value.trim();
    if (!newNs) {
        setNsManageError('New namespace name is required.');
        return;
    }
    if (!/^[A-Za-z0-9._-]+$/.test(newNs)) {
        setNsManageError('Namespace may only contain letters, digits, dot, dash, underscore.');
        return;
    }
    if (newNs === oldNs) {
        exitRenameMode(row);
        return;
    }

    setNsManageError('');
    const saveBtn = row.querySelector('[data-action="rename-save"]');
    saveBtn.disabled = true;
    saveBtn.textContent = 'Saving…';

    const resp = await api.renameNamespace(oldNs, newNs);
    if (!resp.ok) {
        console.error('renameNamespace:', resp.error);
        setNsManageError(resp.error || `Rename failed: ${oldNs} → ${newNs}`);
        saveBtn.disabled = false;
        saveBtn.textContent = 'Save';
        return;
    }

    // An open group stays open under its new name.
    if (state.open.delete(oldNs)) {
        state.open.add(newNs);
        saveOpenGroups();
    }

    Utils.showToast(`Renamed ${oldNs} → ${newNs}`, 'success');
    await refreshNsManageList();
    await loadAll();
}

async function deleteNsFromManage(ns) {
    const entry = nsManageEntries.find(e => e.namespace === ns);
    const keyCount = entry ? entry.key_count : 0;
    const msg = keyCount
        ? `Delete namespace "${ns}"? This removes ${keyCount} key${keyCount === 1 ? '' : 's'} across every language. This cannot be undone.`
        : `Delete namespace "${ns}"? This cannot be undone.`;
    if (!confirm(msg)) return;

    setNsManageError('');
    const resp = await api.deleteNamespace(ns);
    if (!resp.ok) {
        console.error('deleteNamespace:', resp.error);
        setNsManageError(resp.error || `Delete failed: ${ns}`);
        return;
    }

    if (state.open.delete(ns)) saveOpenGroups();
    Utils.showToast(`Deleted namespace ${ns}`, 'success');
    await refreshNsManageList();
    await loadAll();
}

// HTML-entity escaping lives in Utils (utils.js) — every page loads utils.js
// before its page script so we can just alias here. escapeAttr is a semantic
// alias: attribute values and text content need the same five entities.
const escapeHtml = Utils.escapeHtml;
const escapeAttr = Utils.escapeHtml;
function cssEscape(s) {
    return (window.CSS && CSS.escape) ? CSS.escape(s) : String(s).replace(/[^a-zA-Z0-9_-]/g, '\\$&');
}
