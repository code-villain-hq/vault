// Translations page: Locize-style stacked view.
//
// Every (namespace, key) is rendered as a collapsed card. Expanding the card
// reveals every configured language as a horizontal row (flag · name · input ·
// actions), which makes editing one phrase across all languages the natural,
// zero-scroll flow.
//
// Shape of `state.rows`:
//   { ns, key, cells: { [lang]: { value, confidence, translatedBy, missing,
//                                 dirty, saving } } }
//
// Everything else — filters, search, toolbar menus — is pure view state and
// recomputed on each render.

const state = {
    config: null,
    sourceLang: 'en',
    targetLangs: [],
    allLangs: [],       // [source, ...targets]
    namespaces: [],
    confidenceByPair: {},

    rows: [],

    namespace: '',              // '' = all
    search: '',
    langView: 'all',            // 'all' | 'base' | <lang code>
    filterMissing: false,
    filterIssues: false,
    sort: 'alpha',              // alpha | missing | issues
    details: false,             // show confidence/translator inline
    expanded: new Set(),        // "ns::key"
};

document.addEventListener('DOMContentLoaded', async () => {
    await init();
    setupToolbar();
    setupNewKeyModal();
    setupNsManageModal();
    setupKeyboardShortcuts();
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

        renderLangsMenu();
        renderNsMenu();
        updateToolbarLabels();

        await loadAll();
    } catch (err) {
        console.error('Init error:', err);
        Utils.showToast('Error loading translations', 'error');
    }
}

// loadAll fetches every (lang, ns) translation + metadata in parallel and
// folds them into state.rows keyed by (ns, key).
async function loadAll() {
    const list = document.getElementById('keysList');
    Utils.showLoading(list, 'Loading translations…');

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
// Filtering & sorting
// -----------------------------------------------------------------------

function visibleRows() {
    const q = state.search.trim().toLowerCase();
    let rows = state.rows;

    if (state.namespace) rows = rows.filter(r => r.ns === state.namespace);

    if (state.filterMissing) rows = rows.filter(rowHasMissing);
    if (state.filterIssues)  rows = rows.filter(rowHasIssues);

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

    const sorted = rows.slice();
    if (state.sort === 'missing') {
        sorted.sort((a, b) => (countMissing(b) - countMissing(a)) || compareKeys(a, b));
    } else if (state.sort === 'issues') {
        sorted.sort((a, b) => (countIssues(b) - countIssues(a)) || compareKeys(a, b));
    } else {
        sorted.sort(compareKeys);
    }
    return sorted;
}

function compareKeys(a, b) {
    if (a.ns !== b.ns) return a.ns.localeCompare(b.ns);
    return a.key.localeCompare(b.key);
}

function rowHasMissing(row) {
    return state.targetLangs.some(l => row.cells[l].missing);
}
function rowHasIssues(row) {
    return state.targetLangs.some(l => {
        const c = row.cells[l];
        if (c.missing || c.confidence === null) return false;
        return c.confidence < (state.confidenceByPair[l] ?? 0.7);
    });
}
function countMissing(row) {
    return state.targetLangs.filter(l => row.cells[l].missing).length;
}
function countIssues(row) {
    return state.targetLangs.filter(l => {
        const c = row.cells[l];
        if (c.missing || c.confidence === null) return false;
        return c.confidence < (state.confidenceByPair[l] ?? 0.7);
    }).length;
}

// -----------------------------------------------------------------------
// Rendering
// -----------------------------------------------------------------------

// Virtual-list threshold. Below this, we render the whole key list as a plain
// block flow — faster to reason about, and small projects don't benefit from
// virtualization. At or above this, we switch to windowed rendering (see
// renderVirtual) which keeps ~2 screens of cards in the DOM and absolute-
// positions them inside a sized container.
const VIRTUAL_THRESHOLD = 200;

// Overscan: extra cards rendered above/below the viewport so scrolling feels
// smooth. 8 is enough to hide the recompute latency at typical scroll speeds
// without ballooning the DOM.
const VIRTUAL_OVERSCAN = 8;

// Fallback per-row height used until we measure a real collapsed card. The
// actual value is read from the first rendered card and cached; this only
// matters for the very first frame.
let collapsedCardHeight = 52;

// Vertical gap between cards. Matches the `space-y-2` (0.5rem = 8px) class
// on #keysList in classic mode. In virtual mode cards are absolutely
// positioned so the Tailwind rule doesn't apply — we bake the gap into the
// slot height instead so offsets space cards out correctly.
const VIRTUAL_CARD_GAP = 8;

const virtualState = {
    active: false,
    rows: [],           // current visible-rows array (post-filter/sort)
    heights: [],        // per-row measured height
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

    if (rows.length === 0) {
        teardownVirtual();
        list.innerHTML = '';
        list.style.cssText = '';
        empty.classList.remove('hidden');
        return;
    }
    empty.classList.add('hidden');

    if (rows.length < VIRTUAL_THRESHOLD) {
        teardownVirtual();
        list.style.cssText = '';
        list.removeAttribute('data-virtual');
        list.innerHTML = rows.map(renderCard).join('');
        bindCardEvents();
        measureCollapsedHeight();
        return;
    }
    renderVirtual(rows);
}

// measureCollapsedHeight caches the height of a collapsed card so virtual
// mode has a realistic per-row estimate without having to run the layout
// engine on thousands of elements. Called once per classic render so any CSS
// tweak takes effect on the next virtual switch.
function measureCollapsedHeight() {
    const list = document.getElementById('keysList');
    const sample = list.querySelector('.key-card[data-expanded="false"]');
    if (!sample) return;
    const h = sample.getBoundingClientRect().height;
    if (h > 0) collapsedCardHeight = h + VIRTUAL_CARD_GAP;
}

// ----- Virtual list ------------------------------------------------------
//
// Model: every row has an entry in `heights[]`. Collapsed rows use the
// measured `collapsedCardHeight`; expanded rows use whatever we measured
// last time the card was in the window. `offsets[]` is the prefix sum so
// we can translate a scroll position into a row index with a binary search.
//
// Render: on scroll (rAF-throttled) compute [start, end) with overscan, and
// if that range moved, rebuild the DOM inside #keysList with absolutely-
// positioned `.v-card-wrap` elements. After render we re-measure any card
// that looks taller than expected (i.e. expanded ones) and reflow if
// heights changed.

function renderVirtual(rows) {
    const list = document.getElementById('keysList');
    virtualState.active = true;
    virtualState.rows = rows;
    virtualState.heights = rows.map(row =>
        state.expanded.has(rowId(row)) ? (virtualHeightFor(row) || collapsedCardHeight * 3) : collapsedCardHeight,
    );
    rebuildVirtualOffsets();
    virtualState.startIdx = -1;
    virtualState.endIdx = -1;

    list.setAttribute('data-virtual', 'true');
    list.style.position = 'relative';
    list.style.height = virtualState.total + 'px';

    attachVirtualScrollListeners();
    renderVirtualWindow();
}

// Height cache for expanded cards keyed by row id. Survives filter/sort
// changes so re-opening a card doesn't flash a wrong estimate.
const expandedHeightCache = new Map();
function rowId(row) { return `${row.ns}::${row.key}`; }
function virtualHeightFor(row) { return expandedHeightCache.get(rowId(row)); }

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

// findRowAtOffset: binary search the prefix-sum offsets for the row whose
// slab contains y. Returns an index in [0, rows.length].
function findRowAtOffset(y) {
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

// renderVirtualWindow computes the currently-visible row range (with
// overscan), and re-renders #keysList only if that range moved. Cards are
// absolute-positioned via `transform: translateY` so scroll offsets work
// without touching document layout above/below.
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

    const n = virtualState.rows.length;
    let start = findRowAtOffset(Math.max(0, viewportTop));
    let end = findRowAtOffset(Math.max(0, viewportBottom)) + 1;
    start = Math.max(0, start - VIRTUAL_OVERSCAN);
    end = Math.min(n, end + VIRTUAL_OVERSCAN);

    if (start === virtualState.startIdx && end === virtualState.endIdx) return;
    virtualState.startIdx = start;
    virtualState.endIdx = end;

    const parts = new Array(end - start);
    for (let i = start; i < end; i++) {
        const row = virtualState.rows[i];
        const top = virtualState.offsets[i];
        parts[i - start] =
            `<div class="v-card-wrap" data-vidx="${i}" style="transform:translateY(${top}px)">`
            + renderCard(row)
            + `</div>`;
    }
    list.innerHTML = parts.join('');
    bindCardEvents();
    measureExpandedInWindow();
}

// measureExpandedInWindow re-measures the rendered wraps and reflows the
// list if any expanded card's real height differs from what we estimated.
// Collapsed cards are trusted to be `collapsedCardHeight` — measuring them
// would just waste layout passes.
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
            expandedHeightCache.set(rowId(virtualState.rows[idx]), h);
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

function renderCard(row) {
    const id = `${row.ns}::${row.key}`;
    const expanded = state.expanded.has(id);
    const missing = countMissing(row);
    const issues = countIssues(row);
    const translated = state.targetLangs.length - missing;
    const total = state.targetLangs.length;
    const pct = total === 0 ? 100 : Math.round((translated / total) * 100);
    const progressState = translated === 0 ? 'empty' : (translated < total ? 'partial' : 'full');
    const issueBadge = issues > 0 ? `<span class="key-badge-issue">${issues} issue${issues === 1 ? '' : 's'}</span>` : '';

    return `
        <div class="key-card" data-id="${escapeAttr(id)}" data-expanded="${expanded}">
            <div class="key-head" data-action="toggle">
                <span class="key-caret">▶</span>
                <span class="key-ns-tag">${escapeHtml(row.ns)}</span>
                <span class="key-name-wrap">
                    <span class="key-name">${escapeHtml(row.key)}</span>
                    <button class="key-copy-btn" data-action="copy-key"
                            title="Copy key to clipboard"
                            aria-label="Copy key ${escapeAttr(row.key)} to clipboard">📋</button>
                </span>
                <span class="key-meta">
                    ${issueBadge}
                    <span class="key-progress" title="${translated}/${total} translated (${pct}%)">
                        <span class="key-progress-fill" data-state="${progressState}" style="width: ${pct}%"></span>
                    </span>
                    <span class="key-progress-label">${translated}/${total}</span>
                    <button class="key-delete-btn" data-action="delete-key"
                            title="Delete this key (removes it from every language)"
                            aria-label="Delete key ${escapeAttr(row.key)}">🗑️</button>
                </span>
            </div>
            ${expanded ? `<div class="key-body">
                <div class="key-context-panel px-1 mb-3">
                    <label class="block text-[11px] font-medium text-slate-500 dark:text-slate-400 mb-1">Translator context <span class="font-normal opacity-80">(where this string appears — optional, included in LLM prompts)</span></label>
                    <textarea class="key-context-input w-full min-h-[2.5rem] px-2 py-1.5 rounded-md border border-slate-300 dark:border-slate-600 bg-white dark:bg-slate-900 text-sm text-slate-800 dark:text-slate-100 placeholder:text-slate-400 dark:placeholder:text-slate-500" rows="2" placeholder="e.g. Error banner on checkout; keep formal">${escapeHtml(row.keyContext || '')}</textarea>
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

// Which languages to show inside an expanded key card, honoring the langView
// picker (all / base only / a specific language).
function pickVisibleLangs() {
    if (state.langView === 'base') return [state.sourceLang];
    if (state.langView === 'all')  return state.allLangs;
    // A specific language: show source + that one so the user always has the
    // base text next to what they're editing.
    const pick = state.langView;
    if (pick === state.sourceLang) return [state.sourceLang];
    return [state.sourceLang, pick];
}

function renderLangRow(row, lang) {
    const cell = row.cells[lang] || { value: '', missing: true, confidence: null };
    const info = Utils.langInfo(lang);
    const isSource = lang === state.sourceLang;
    const threshold = state.confidenceByPair[lang] ?? 0.7;
    const isLow = !isSource && cell.confidence !== null && cell.confidence < threshold;

    const inputAttrs = [
        'class="lang-input"',
        `data-missing="${cell.missing}"`,
        `data-low="${isLow}"`,
        'rows="1"',
        cell.saving ? 'data-saving="true"' : '',
        cell.missing ? 'placeholder="— not translated —"' : '',
    ].filter(Boolean).join(' ');

    // The confidence chip is always rendered next to the language code so
    // reviewers can spot a low-confidence translation at a glance — without
    // needing to flip the Details toggle. The toggle still exists; it just
    // controls the richer strip below the input (translator + timestamp).
    const chip = renderConfidenceChip(cell, isSource, threshold);
    const details = state.details && !isSource ? renderDetails(cell) : '';

    // Source-row action:
    //   - If the source cell is missing AND at least one target cell has a
    //     value, offer a ✨ "back-fill from translation" button: the backend
    //     runs the LLM with (source=that target lang, target=sourceLang) using
    //     the existing target value as the prompt input. No-op if every
    //     target is also empty.
    //   - Otherwise: just 📋 copy.
    let actions;
    if (isSource) {
        const backFillSource = cell.missing ? pickBackFillSource(row) : null;
        actions = backFillSource
            ? `
                <button class="lang-icon-btn lang-icon-btn--translate" data-action="fill-source"
                        data-from="${escapeAttr(backFillSource.lang)}"
                        title="Auto-fill ${escapeAttr(state.sourceLang)} from the existing ${escapeAttr(backFillSource.lang)} translation">
                    <span class="lang-icon-glyph">✨</span>
                </button>
                <button class="lang-icon-btn" data-action="copy-source" title="Copy to clipboard">📋</button>
            `
            : `<button class="lang-icon-btn" data-action="copy-source" title="Copy to clipboard">📋</button>`;
    } else {
        actions = `
            <button class="lang-icon-btn lang-icon-btn--translate" data-action="translate" title="Auto-translate from ${escapeAttr(state.sourceLang)}">
                <span class="lang-icon-glyph">✨</span>
            </button>
            <button class="lang-icon-btn" data-action="clear" title="Clear translation">✕</button>
        `;
    }

    return `
        <div class="lang-row" data-lang="${escapeAttr(lang)}">
            <div class="lang-flag" aria-hidden="true">${info.flag}</div>
            <div class="lang-label">
                <div class="lang-name">${escapeHtml(info.name)}</div>
                <div class="lang-code">${escapeHtml(lang)}</div>
                ${chip}
                ${isSource ? '<span class="lang-source-badge">base</span>' : ''}
            </div>
            <div class="lang-input-wrap">
                <textarea ${inputAttrs}>${escapeHtml(cell.value)}</textarea>
                ${details}
            </div>
            <div class="lang-actions">${actions}</div>
        </div>
    `;
}

// renderConfidenceChip returns the always-on confidence indicator for a
// non-source row. Tone is derived from the pair's threshold:
//   - `high`    (≥ threshold): green
//   - `medium`  (≥ 0.5 but below threshold): amber
//   - `low`     (< 0.5): red
//   - `unknown` (null / missing): no chip
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
    return `<span class="lang-confidence-chip" data-tone="${human ? 'human' : tone}" title="${escapeAttr(title)}">${escapeHtml(label)}</span>`;
}

function renderDetails(cell) {
    const bits = [];
    if (cell.translatedBy) bits.push(`<span class="lang-detail-pill">by ${escapeHtml(cell.translatedBy)}</span>`);
    if (cell.timestamp) {
        const dt = Utils.formatDate(cell.timestamp);
        if (dt !== 'N/A') bits.push(`<span class="lang-detail-pill">${escapeHtml(dt)}</span>`);
    }
    return bits.length ? `<div class="lang-details">${bits.join('')}</div>` : '';
}

function updateSummary(rows) {
    const summary = document.getElementById('transSummary');
    let filled = 0;
    let total = 0;
    for (const r of rows) {
        for (const l of state.targetLangs) {
            total++;
            if (!r.cells[l].missing) filled++;
        }
    }
    const pct = total === 0 ? 100 : Math.round((filled / total) * 100);
    summary.textContent = `${rows.length} key${rows.length === 1 ? '' : 's'} · ${filled}/${total} translated (${pct}%)`;
}

// -----------------------------------------------------------------------
// Event binding for the rendered cards
// -----------------------------------------------------------------------

function bindCardEvents() {
    const list = document.getElementById('keysList');

    list.querySelectorAll('[data-action="toggle"]').forEach(el => {
        el.addEventListener('click', (e) => {
            // Let clicks inside textareas/buttons pass through.
            if (e.target.closest('textarea, button')) return;
            const card = el.closest('.key-card');
            toggleCard(card.dataset.id);
        });
    });

    list.querySelectorAll('[data-action="delete-key"]').forEach(btn => {
        btn.addEventListener('click', (e) => {
            e.stopPropagation();
            const card = btn.closest('.key-card');
            if (!card) return;
            deleteKey(card.dataset.id, btn);
        });
    });

    list.querySelectorAll('[data-action="copy-key"]').forEach(btn => {
        btn.addEventListener('click', (e) => {
            e.stopPropagation();
            const card = btn.closest('.key-card');
            if (!card) return;
            copyKey(card.dataset.id, btn);
        });
    });

    list.querySelectorAll('.key-card[data-expanded="true"]').forEach(card => {
        bindExpandedCard(card);
    });
}

async function deleteKey(id, btn) {
    const row = findRow(id);
    if (!row) return;
    const confirmMsg = `Delete "${row.key}" from namespace "${row.ns}"?\n\nThis will remove the key from every language and re-export each affected JSON file. This cannot be undone.`;
    if (!confirm(confirmMsg)) return;

    btn.disabled = true;
    const original = btn.textContent;
    btn.textContent = '…';

    try {
        const res = await api.deleteKey(row.key, row.ns);
        if (!res.ok) {
            Utils.showToast(`Delete failed: ${res.error || 'unknown error'}`, 'error');
            btn.disabled = false;
            btn.textContent = original;
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
        btn.textContent = original;
    }
}

// copyKey writes the key name to the clipboard so the user can paste it
// straight into application code (t('common.welcome'), …). We copy just the
// key — not `ns.key` — because the namespace is part of the row's metadata,
// not of the identifier most frameworks expect. Briefly flips the button to
// a check-mark so the click produces visible feedback even when the toast
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
    const original = btn.textContent;
    btn.textContent = '✓';
    btn.setAttribute('data-copied', 'true');
    Utils.showToast(`Copied "${row.key}"`, 'success');
    setTimeout(() => {
        btn.textContent = original;
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
        const idx = virtualState.rows.findIndex(r => rowId(r) === id);
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
        Utils.showToast(`✨ Filled ${sourceLang} from ${fromLang}`, 'success');
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
        // amber confidence chip — no toast needed, the chip already tells
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

    // Refresh the always-visible confidence chip so a newly-translated cell
    // shows its score without needing a full re-render.
    const label = rowEl.querySelector('.lang-label');
    if (label) {
        const existingChip = label.querySelector('.lang-confidence-chip');
        if (existingChip) existingChip.remove();
        const chipHtml = renderConfidenceChip(cell, lang === state.sourceLang, threshold);
        if (chipHtml) {
            // Insert after .lang-code so the DOM order matches renderLangRow.
            const code = label.querySelector('.lang-code');
            (code || label).insertAdjacentHTML('afterend', chipHtml);
        }
    }

    // Rebuild details strip in-place if it's visible.
    if (state.details && lang !== state.sourceLang) {
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
}

// -----------------------------------------------------------------------
// Toolbar
// -----------------------------------------------------------------------

function setupToolbar() {
    document.getElementById('searchInput').addEventListener('input', Utils.debounce((e) => {
        state.search = e.target.value;
        render();
    }, 150));

    document.getElementById('detailsToggleBtn').addEventListener('click', (e) => {
        state.details = !state.details;
        e.currentTarget.setAttribute('aria-expanded', String(state.details));
        render();
    });

    document.getElementById('refreshBtn').addEventListener('click', loadAll);

    document.getElementById('fillAllMissingBtn').addEventListener('click', fillAllMissing);
    document.getElementById('newKeyBtn').addEventListener('click', () => openNewKeyModal({ mode: 'key' }));
    // "New namespace" lives in the namespace-filter dropdown (see renderNsMenu);
    // the toolbar button was removed to avoid a double entry point.

    document.getElementById('filterMissing').addEventListener('change', (e) => {
        state.filterMissing = e.target.checked;
        updateFiltersBadge();
        render();
    });
    document.getElementById('filterIssues').addEventListener('change', (e) => {
        state.filterIssues = e.target.checked;
        updateFiltersBadge();
        render();
    });

    document.querySelectorAll('[data-menu-toggle]').forEach(btn => {
        btn.addEventListener('click', (e) => {
            e.stopPropagation();
            const key = btn.dataset.menuToggle;
            toggleMenu(key, btn);
        });
    });

    document.querySelectorAll('[data-sort]').forEach(btn => {
        btn.addEventListener('click', () => {
            state.sort = btn.dataset.sort;
            closeAllMenus();
            updateToolbarLabels();
            render();
        });
    });

    document.addEventListener('click', (e) => {
        if (!e.target.closest('.tx-menu')) closeAllMenus();
    });
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
    document.querySelectorAll('[data-menu-toggle]').forEach(b => {
        // Keep Details' toggled state sticky since it's a toggle, not a popover.
        if (b.id !== 'detailsToggleBtn') b.setAttribute('aria-expanded', 'false');
    });
}

function renderLangsMenu() {
    const panel = document.getElementById('langsMenuPanel');
    const items = [
        { value: 'all',  label: 'All languages', icon: '🌐' },
        { value: 'base', label: 'Base only',     icon: '📌' },
    ];
    for (const lang of state.allLangs) {
        const info = Utils.langInfo(lang);
        items.push({ value: lang, label: `${info.name} (${lang})`, icon: info.flag });
    }
    panel.innerHTML = items.map(item => `
        <button class="tx-menu-item" data-lang-view="${escapeAttr(item.value)}" aria-selected="${state.langView === item.value}">
            <span>${item.icon}</span>
            <span>${escapeHtml(item.label)}</span>
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

function renderNsMenu() {
    const panel = document.getElementById('nsMenuPanel');
    const items = [{ value: '', label: 'All namespaces' }, ...state.namespaces.map(ns => ({ value: ns, label: ns }))];
    panel.innerHTML = items.map(item => `
        <button class="tx-menu-item" data-ns="${escapeAttr(item.value)}" aria-selected="${state.namespace === item.value}">
            <span>📂</span>
            <span>${escapeHtml(item.label)}</span>
        </button>
    `).join('') + `
        <div class="tx-menu-divider"></div>
        <button class="tx-menu-item tx-menu-item--action" data-ns-action="new">
            <span>➕</span>
            <span>New namespace…</span>
        </button>
        <button class="tx-menu-item tx-menu-item--action" data-ns-action="manage">
            <span>⚙️</span>
            <span>Manage namespaces…</span>
        </button>
    `;
    panel.querySelectorAll('[data-ns]').forEach(btn => {
        btn.addEventListener('click', () => {
            state.namespace = btn.dataset.ns;
            renderNsMenu();
            updateToolbarLabels();
            closeAllMenus();
            render();
        });
    });
    const newBtn = panel.querySelector('[data-ns-action="new"]');
    if (newBtn) {
        newBtn.addEventListener('click', () => {
            closeAllMenus();
            openNewKeyModal({ mode: 'namespace' });
        });
    }
    const manageBtn = panel.querySelector('[data-ns-action="manage"]');
    if (manageBtn) {
        manageBtn.addEventListener('click', () => {
            closeAllMenus();
            openNsManageModal();
        });
    }
}

function updateToolbarLabels() {
    const langsLabel = document.getElementById('langsBtnLabel');
    const langsIcon  = document.getElementById('langsBtnIcon');
    if (state.langView === 'all') {
        langsLabel.textContent = 'All languages';
        langsIcon.textContent = '🌐';
    } else if (state.langView === 'base') {
        langsLabel.textContent = 'Base only';
        langsIcon.textContent = '📌';
    } else {
        const info = Utils.langInfo(state.langView);
        langsLabel.textContent = `${info.name}`;
        langsIcon.textContent = info.flag;
    }

    document.getElementById('nsBtnLabel').textContent = state.namespace || 'All namespaces';

    const sortLabels = { alpha: 'Alphabetically', missing: 'Missing first', issues: 'Issues first' };
    document.getElementById('sortBtnLabel').textContent = sortLabels[state.sort] || 'Alphabetically';
}

function updateFiltersBadge() {
    const badge = document.getElementById('filtersBadge');
    const n = (state.filterMissing ? 1 : 0) + (state.filterIssues ? 1 : 0);
    if (n === 0) {
        badge.classList.add('hidden');
    } else {
        badge.classList.remove('hidden');
        badge.textContent = String(n);
    }
}

// -----------------------------------------------------------------------
// Bulk fill
// -----------------------------------------------------------------------

async function fillAllMissing() {
    const rows = visibleRows().filter(rowHasMissing);
    if (rows.length === 0) {
        Utils.showToast('Nothing missing in the current view', 'info');
        return;
    }
    if (!confirm(`Auto-translate missing cells for ${rows.length} key${rows.length === 1 ? '' : 's'}?`)) return;

    // Group by (target_lang, namespace) and use the batch endpoint — much
    // faster than N /translate/single calls.
    const groups = new Map();
    for (const r of rows) {
        for (const lang of state.targetLangs) {
            if (!r.cells[lang].missing) continue;
            const k = `${lang}|${r.ns}`;
            if (!groups.has(k)) groups.set(k, { lang, ns: r.ns });
        }
    }

    const btn = document.getElementById('fillAllMissingBtn');
    btn.disabled = true;
    const original = btn.textContent;
    btn.textContent = `⏳ Filling ${groups.size} group${groups.size === 1 ? '' : 's'}…`;

    try {
        for (const { lang, ns } of groups.values()) {
            const res = await api.translateMissing([lang], state.sourceLang, ns);
            if (!res.ok) {
                Utils.showToast(`Batch failed ${lang}/${ns}: ${res.error}`, 'error');
            }
        }
        Utils.showToast('✓ Translation pass complete', 'success');
        await loadAll();
    } finally {
        btn.disabled = false;
        btn.textContent = original;
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
        sel.value = state.namespace && existing.includes(state.namespace) ? state.namespace : existing[0];
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

        closeNewKeyModal();
        renderNsMenu();
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
    list.innerHTML = `<div class="py-8 text-center text-xs text-slate-500 dark:text-slate-400">Loading…</div>`;
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
        <div class="ns-manage-row flex items-center gap-3 px-3 py-2 rounded-lg border border-slate-200 dark:border-slate-700 bg-slate-50 dark:bg-slate-900/40"
             data-ns="${escapeAttr(entry.namespace)}">
            <span class="text-lg">📂</span>
            <div class="flex-1 min-w-0">
                <div class="ns-manage-name font-mono text-sm text-slate-900 dark:text-white truncate" data-role="display">
                    ${escapeHtml(entry.namespace)}
                </div>
                <input type="text" value="${escapeAttr(entry.namespace)}" data-role="edit"
                       class="hidden w-full px-2 py-1 rounded border border-slate-300 dark:border-slate-600 dark:bg-slate-700 dark:text-white text-sm font-mono">
                <div class="text-[11px] text-slate-500 dark:text-slate-400 mt-0.5">
                    ${entry.key_count} key${entry.key_count === 1 ? '' : 's'}
                    · ${entry.translation_count} translation${entry.translation_count === 1 ? '' : 's'}
                </div>
            </div>
            <div class="flex items-center gap-1" data-role="actions-view">
                <button class="px-2 py-1 rounded-md text-xs font-medium bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-600 text-slate-700 dark:text-slate-200 hover:bg-slate-100 dark:hover:bg-slate-700"
                        data-action="rename">Rename</button>
                <button class="px-2 py-1 rounded-md text-xs font-medium bg-white dark:bg-slate-800 border border-red-300 dark:border-red-700 text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-900/30"
                        data-action="delete">Delete</button>
            </div>
            <div class="hidden items-center gap-1" data-role="actions-edit">
                <button class="px-2 py-1 rounded-md text-xs font-medium bg-indigo-600 hover:bg-indigo-700 text-white"
                        data-action="rename-save">Save</button>
                <button class="px-2 py-1 rounded-md text-xs font-medium bg-slate-200 dark:bg-slate-700 text-slate-700 dark:text-slate-200 hover:bg-slate-300 dark:hover:bg-slate-600"
                        data-action="rename-cancel">Cancel</button>
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

    // If the user was filtered by the old namespace, carry the filter over.
    if (state.namespace === oldNs) state.namespace = newNs;

    Utils.showToast(`Renamed ${oldNs} → ${newNs}`, 'success');
    await refreshNsManageList();
    await loadAll();
    renderNsMenu();
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

    if (state.namespace === ns) state.namespace = '';
    Utils.showToast(`Deleted namespace ${ns}`, 'success');
    await refreshNsManageList();
    await loadAll();
    renderNsMenu();
}

// HTML-entity escaping lives in Utils (utils.js) — every page loads utils.js
// before its page script so we can just alias here. escapeAttr is a semantic
// alias: attribute values and text content need the same five entities.
const escapeHtml = Utils.escapeHtml;
const escapeAttr = Utils.escapeHtml;
function cssEscape(s) {
    return (window.CSS && CSS.escape) ? CSS.escape(s) : String(s).replace(/[^a-zA-Z0-9_-]/g, '\\$&');
}
