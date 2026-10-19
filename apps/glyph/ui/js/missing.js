// Review page: per-row confirm flow.
//
// Lifecycle for a single row:
//   pending  → (click "Suggest") → previewing → (LLM returns) → proposed
//          ↘ (click "Write")  → proposed (empty textarea, translatedBy=human)
//     ├ Accept   → PUT /api/translations  → gone from list
//     ├ Edit     → user tweaks textarea → Accept path
//     └ Reject   → back to pending (nothing saved)
//
// Bulk actions ("Suggest visible", "Suggest <lang> (N)") run previews in
// parallel (capped per language so the LLM is not flooded). Language groups
// start collapsed; expand a group to see its rows. An "Accept all" button
// appears in the toolbar when there are proposals.
//
// Backend contract: /api/translate/single accepts `dry_run: true` which
// produces the same result payload without touching the DB.

const state = {
    sourceLang: 'en',
    targetLangs: [],
    namespaces: [],
    target: '',
    namespace: '',
    search: '',
    // rows[] { id, lang, ns, key, source, status, proposal, confidence,
    //         translatedBy, originalProposal, skipped }
    //   status: 'pending' | 'previewing' | 'proposed' | 'saving' | 'error'
    rows: [],
    // Per-language section: expanded[lang] === true means the group body is
    // open. Absent/false ⇒ collapsed (default), so the page does not render
    // every row at once on load.
    expanded: {},
    activeIdx: 0,
    config: null,
};

// Per-language-group windowed list: when a group has many rows, only a
// slice is mounted in the DOM (same idea as the Translations page). Threshold
// is lower than Translations because review rows are heavier (grid + actions).
const REVIEW_VIRT_THRESHOLD = 120;
const REVIEW_VIRT_OVERSCAN = 6;
/** Max concurrent dry-run translations per language in bulk Suggest (server-friendly). */
const MAX_SUGGEST_PER_LANG = 50;

/** @type {Map<string, { items: {row: *, idx: number}[], heights: number[], offsets: number[], total: number, startIdx: number, endIdx: number, bodyEl: HTMLElement | null }>} */
const reviewVirtualByLang = new Map();
let reviewVirtualGlobalHandler = null;
let reviewVirtualScrollAttached = false;
let reviewVirtualRafPending = false;

document.addEventListener('DOMContentLoaded', async () => {
    await initReview();
    setupEventHandlers();
    setupKeyboardShortcuts();
});

async function initReview() {
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

        const allLangs = (langRes.ok ? langRes.data : [])
            .map(l => l.code || l).filter(Boolean);
        state.targetLangs = allLangs.filter(c => c !== state.sourceLang).sort();

        // The language filter also offers the source lang so reviewers can
        // focus on back-fill rows (keys missing in English but present in
        // some target). The loader treats `state.sourceLang` specially and
        // fetches from /missing-source instead of /missing for that entry.
        const targetSel = document.getElementById('targetLanguage');
        const srcOpt = `<option value="${escapeAttr(state.sourceLang)}">${escapeHtml(state.sourceLang)} (source — back-fill)</option>`;
        targetSel.innerHTML = '<option value="">All languages</option>'
            + srcOpt
            + state.targetLangs.map(l => `<option value="${escapeAttr(l)}">${escapeHtml(l)}</option>`).join('');

        state.namespaces = nsRes.ok ? (nsRes.data.namespaces || []) : [];
        const nsSel = document.getElementById('namespaceFilter');
        nsSel.innerHTML = '<option value="">All namespaces</option>'
            + state.namespaces.map(ns => `<option value="${escapeAttr(ns)}">${escapeHtml(ns)}</option>`).join('');

        const params = new URLSearchParams(window.location.search);
        const queryTarget = params.get('target');
        const validTargets = [state.sourceLang, ...state.targetLangs];
        if (queryTarget && validTargets.includes(queryTarget)) {
            state.target = queryTarget;
            targetSel.value = queryTarget;
        }

        await loadRows();
    } catch (err) {
        console.error('Init error:', err);
        Utils.showToast('Error initializing', 'error');
    }
}

async function loadRows() {
    const container = document.getElementById('reviewContainer');
    Utils.showLoading(container, 'Loading missing translations…');

    // A filter of `''` means "everything", so we pull both the normal
    // forward rows (missing in each target) AND the reverse rows (missing
    // in the source lang itself). When the user filters down to a single
    // language, we only hit the endpoint matching that direction — keeps
    // the load proportional to what's actually shown.
    const allLangs = state.target ? [state.target] : [state.sourceLang, ...state.targetLangs];
    const namespaces = state.namespace ? [state.namespace] : (state.namespaces.length ? state.namespaces : ['translation']);

    const forwardTasks = [];
    const reverseTasks = [];
    for (const lang of allLangs) {
        for (const ns of namespaces) {
            if (lang === state.sourceLang) {
                reverseTasks.push({ lang, ns });
            } else {
                forwardTasks.push({ lang, ns });
            }
        }
    }

    try {
        const [forwardResults, reverseResults] = await Promise.all([
            Promise.all(forwardTasks.map(t => api.getMissingTranslations(t.lang, state.sourceLang, t.ns))),
            Promise.all(reverseTasks.map(t => api.getMissingSource(t.lang, t.ns))),
        ]);

        const rows = [];
        forwardTasks.forEach((t, i) => {
            const r = forwardResults[i];
            if (!r.ok) return;
            for (const item of (r.data || [])) {
                rows.push({
                    id: `${t.lang}|${t.ns}|${item.key}`,
                    lang: t.lang,
                    ns: t.ns,
                    key: item.key,
                    source: item.source || '',
                    // Normal forward rows translate from the global source lang.
                    sourceLang: state.sourceLang,
                    status: 'pending',
                    proposal: '',
                    confidence: null,
                    translatedBy: '',
                    originalProposal: '',
                    skipped: false,
                });
            }
        });
        reverseTasks.forEach((t, i) => {
            const r = reverseResults[i];
            if (!r.ok) return;
            for (const item of (r.data || [])) {
                rows.push({
                    id: `${t.lang}|${t.ns}|${item.key}`,
                    // The key is missing in the source lang, so `lang` is
                    // the one we're filling (source). `sourceLang` is the
                    // sampled lang whose value we use as the prompt input.
                    lang: t.lang,
                    ns: t.ns,
                    key: item.key,
                    source: item.source || '',
                    sourceLang: item.source_lang || '',
                    status: 'pending',
                    proposal: '',
                    confidence: null,
                    translatedBy: '',
                    originalProposal: '',
                    skipped: false,
                });
            }
        });
        rows.sort((a, b) => {
            if (a.lang !== b.lang) return a.lang.localeCompare(b.lang);
            if (a.ns !== b.ns) return a.ns.localeCompare(b.ns);
            return a.key.localeCompare(b.key);
        });
        state.rows = rows;
        state.activeIdx = 0;
        state.expanded = {};
        renderReview();
    } catch (err) {
        console.error('Load error:', err);
        Utils.showToast('Error loading missing translations', 'error');
    }
}

// -------------------------------------------------------------------------
// Filtering
// -------------------------------------------------------------------------

function visibleRows() {
    const q = state.search.trim().toLowerCase();
    return state.rows.filter(r => {
        if (r.skipped) return false;
        if (!q) return true;
        return r.key.toLowerCase().includes(q)
            || r.source.toLowerCase().includes(q)
            || (r.proposal || '').toLowerCase().includes(q);
    });
}

// proposedRows returns every proposed row currently on screen. We scope this
// to `visibleRows()` (not `state.rows`) so the "Accept visible" toolbar button
// and the per-language "Accept N proposals" group header always operate on
// exactly the same row set the user can see — otherwise a stale search filter
// would let a bulk-accept apply to rows that aren't even rendered.
function proposedRows() {
    return visibleRows().filter(r => r.status === 'proposed');
}

// -------------------------------------------------------------------------
// Virtual list (per language group)
// -------------------------------------------------------------------------

function teardownReviewVirtualizers() {
    if (reviewVirtualGlobalHandler) {
        window.removeEventListener('scroll', reviewVirtualGlobalHandler);
        window.removeEventListener('resize', reviewVirtualGlobalHandler);
        reviewVirtualGlobalHandler = null;
    }
    reviewVirtualScrollAttached = false;
    reviewVirtualRafPending = false;
    reviewVirtualByLang.clear();
}

function reviewRowEstimatedHeight(row) {
    if (row.status === 'proposed' || row.status === 'saving') return 156;
    return 54;
}

function rebuildReviewVirtualOffsets(st) {
    st.offsets = [];
    let acc = 0;
    for (let i = 0; i < st.heights.length; i++) {
        st.offsets[i] = acc;
        acc += st.heights[i];
    }
    st.total = acc;
}

function findRowAtOffsetLocal(offsets, y) {
    if (!offsets.length) return 0;
    let lo = 0;
    let hi = offsets.length - 1;
    while (lo < hi) {
        const mid = (lo + hi + 1) >>> 1;
        if (offsets[mid] <= y) lo = mid;
        else hi = mid - 1;
    }
    return lo;
}

function paintReviewVirtualWindow(lang) {
    const st = reviewVirtualByLang.get(lang);
    if (!st?.bodyEl) return;

    const active = document.activeElement;
    if (active && st.bodyEl.contains(active) && active.tagName === 'TEXTAREA') return;

    const rect = st.bodyEl.getBoundingClientRect();
    const viewportTop = -rect.top;
    const viewportBottom = viewportTop + window.innerHeight;
    const n = st.items.length;
    let start = findRowAtOffsetLocal(st.offsets, Math.max(0, viewportTop));
    let end = findRowAtOffsetLocal(st.offsets, Math.max(0, viewportBottom)) + 1;
    start = Math.max(0, start - REVIEW_VIRT_OVERSCAN);
    end = Math.min(n, end + REVIEW_VIRT_OVERSCAN);

    if (start === st.startIdx && end === st.endIdx) return;
    st.startIdx = start;
    st.endIdx = end;

    const parts = [];
    for (let i = start; i < end; i++) {
        const { row, idx } = st.items[i];
        const top = st.offsets[i];
        parts.push(
            `<div class="v-review-wrap" data-review-vidx="${i}" style="transform:translateY(${top}px)">`
            + renderRow(row, idx)
            + '</div>',
        );
    }
    st.bodyEl.innerHTML = parts.join('');
    bindReviewRowsIn(st.bodyEl);
}

function initReviewVirtualForLang(lang, items) {
    let bodyEl = null;
    document.querySelectorAll('.review-virtual-body').forEach(el => {
        if (el.dataset.reviewVirt === lang) bodyEl = el;
    });
    if (!bodyEl) return;
    const heights = items.map(({ row }) => reviewRowEstimatedHeight(row));
    const st = {
        items,
        heights,
        offsets: [],
        total: 0,
        startIdx: -1,
        endIdx: -1,
        bodyEl,
    };
    rebuildReviewVirtualOffsets(st);
    reviewVirtualByLang.set(lang, st);
    bodyEl.style.height = `${st.total}px`;
    paintReviewVirtualWindow(lang);
}

function attachReviewVirtualScrollIfNeeded() {
    if (reviewVirtualByLang.size === 0 || reviewVirtualScrollAttached) return;
    reviewVirtualScrollAttached = true;
    reviewVirtualGlobalHandler = () => {
        if (reviewVirtualRafPending) return;
        reviewVirtualRafPending = true;
        requestAnimationFrame(() => {
            reviewVirtualRafPending = false;
            for (const lang of reviewVirtualByLang.keys()) {
                paintReviewVirtualWindow(lang);
            }
        });
    };
    window.addEventListener('scroll', reviewVirtualGlobalHandler, { passive: true });
    window.addEventListener('resize', reviewVirtualGlobalHandler);
}

function refreshReviewVirtualLang(lang) {
    const st = reviewVirtualByLang.get(lang);
    if (!st) return;
    st.heights = st.items.map(({ row }) => reviewRowEstimatedHeight(row));
    rebuildReviewVirtualOffsets(st);
    if (st.bodyEl) st.bodyEl.style.height = `${st.total}px`;
    st.startIdx = -1;
    st.endIdx = -1;
    paintReviewVirtualWindow(lang);
}

function ensureVirtualReviewRowVisible(idx) {
    const row = findRowByIdx(idx);
    if (!row) return;
    const st = reviewVirtualByLang.get(row.lang);
    if (!st?.bodyEl) return;
    const local = st.items.findIndex(x => x.idx === idx);
    if (local < 0) return;
    const rowTopDoc = window.scrollY + st.bodyEl.getBoundingClientRect().top + st.offsets[local];
    const targetY = rowTopDoc - window.innerHeight * 0.22;
    window.scrollTo({ top: Math.max(0, targetY), behavior: 'auto' });
    for (const l of reviewVirtualByLang.keys()) paintReviewVirtualWindow(l);
    const target = document.querySelector(`.review-row[data-idx="${idx}"]`);
    if (target) {
        document.querySelectorAll('.review-row').forEach(el => el.removeAttribute('data-active'));
        target.setAttribute('data-active', 'true');
    }
}

// -------------------------------------------------------------------------
// Rendering
// -------------------------------------------------------------------------

function renderReview() {
    teardownReviewVirtualizers();

    const container = document.getElementById('reviewContainer');
    const visible = visibleRows();
    updateSummary(visible);
    updateAcceptAllBtn();

    if (visible.length === 0) {
        container.innerHTML = `
            <div class="p-12 text-center text-slate-500 dark:text-slate-400">
                <p class="text-base">🎉 Nothing missing in the current view.</p>
                <p class="text-xs mt-2">Try clearing the filters above to see other languages or namespaces.</p>
            </div>
        `;
        return;
    }

    const byLang = new Map();
    visible.forEach((row, idx) => {
        if (!byLang.has(row.lang)) byLang.set(row.lang, []);
        byLang.get(row.lang).push({ row, idx });
    });

    container.innerHTML = [...byLang.entries()].map(([lang, items]) => {
        const collapsed = !state.expanded[lang];
        const proposedInGroup = items.filter(i => i.row.status === 'proposed').length;
        const useVirt = !collapsed && items.length >= REVIEW_VIRT_THRESHOLD;
        const rowsHtml = collapsed
            ? ''
            : useVirt
                ? `<div class="review-virtual-body" data-review-virt="${escapeAttr(lang)}" style="position:relative;contain:strict"></div>`
                : items.map(({ row, idx }) => renderRow(row, idx)).join('');
        const groupAcceptBtn = proposedInGroup > 0
            ? `<button class="review-btn review-btn-accept" data-action="accept-lang" data-lang="${escapeAttr(lang)}" title="Accept all ${proposedInGroup} proposed translations for ${escapeAttr(lang)}">
                   ✓ Accept ${proposedInGroup} proposal${proposedInGroup === 1 ? '' : 's'}
               </button>`
            : '';
        return `
            <div class="review-group" data-group-lang="${escapeAttr(lang)}">
                <div class="review-group-header" data-collapsed="${collapsed}" data-action="toggle-group" data-lang="${escapeAttr(lang)}">
                    <span class="review-group-caret">▾</span>
                    <span class="font-mono">${escapeHtml(lang)}</span>
                    <span class="text-slate-500 dark:text-slate-400 font-normal">(${items.length} missing${proposedInGroup > 0 ? `, ${proposedInGroup} proposed` : ''})</span>
                    <div class="ml-auto flex gap-1">
                        ${groupAcceptBtn}
                        <button class="review-btn" data-action="translate-lang" data-lang="${escapeAttr(lang)}" title="Generate proposals for pending keys in ${escapeAttr(lang)} (max ${MAX_SUGGEST_PER_LANG} per run; nothing is saved until you accept)">
                            ✨ Suggest ${escapeHtml(lang)} (${items.length})
                        </button>
                    </div>
                </div>
                <div class="review-group-body">${rowsHtml}</div>
            </div>
        `;
    }).join('');

    for (const [lang, items] of byLang.entries()) {
        if (!state.expanded[lang]) continue;
        if (items.length >= REVIEW_VIRT_THRESHOLD) {
            initReviewVirtualForLang(lang, items);
        }
    }
    attachReviewVirtualScrollIfNeeded();

    bindReviewGroupHeaders(container);
    container.querySelectorAll('.review-group-body').forEach(body => {
        if (body.querySelector('.review-virtual-body')) return;
        bindReviewRowsIn(body);
    });
    syncActiveRow();
}

function renderRow(row, idx) {
    if (row.status === 'proposed' || row.status === 'saving') {
        return renderProposedRow(row, idx);
    }
    const busy = row.status === 'previewing';
    const errTone = row.status === 'error' ? ' data-state="error"' : '';
    // When the row's source lang ISN'T the global source (en), it's a
    // back-fill row — the key is missing in English and we're going to
    // translate FROM whatever existing lang we sampled. Flagging that
    // inline avoids confusion when the source column shows Swedish text
    // inside a row that's grouped under "en".
    const isReverse = row.sourceLang && row.sourceLang !== state.sourceLang;
    const dirBadge = isReverse
        ? `<span class="review-direction-badge" title="English is missing; we'll back-translate this ${escapeAttr(row.sourceLang)} value into ${escapeAttr(row.lang)}.">${escapeHtml(row.sourceLang)} → ${escapeHtml(row.lang)}</span>`
        : '';
    return `
        <div class="review-row"${errTone} data-idx="${idx}" data-id="${escapeAttr(row.id)}" ${isReverse ? 'data-reverse="true"' : ''}>
            <div class="review-key" title="${escapeAttr(row.ns)}.${escapeAttr(row.key)}">
                <span class="key-ns-tag">${escapeHtml(row.ns)}</span>
                <span class="review-key-name">${escapeHtml(row.key)}</span>
                ${dirBadge}
            </div>
            <div class="review-source" title="${escapeAttr(row.source)}">${escapeHtml(row.source)}</div>
            <div class="review-actions">
                <button class="review-btn" data-action="suggest-row" ${busy ? 'disabled' : ''} title="Generate a proposal (nothing is saved until you confirm)">
                    ${busy ? '⏳ suggesting…' : '✨ Suggest'}
                </button>
                <button class="review-btn review-btn-secondary" data-action="write-row" ${busy ? 'disabled' : ''} title="Type a translation yourself (nothing is saved until you Accept)">
                    ✎ Write
                </button>
                <button class="review-btn review-btn-secondary" data-action="skip-row" title="Hide from this view (until reload)">✕</button>
            </div>
        </div>
    `;
}

// The proposed-row takes more vertical space because the editable textarea
// for the suggestion lives here. Layout:
//
//   [key]  [source]                                               [skip]
//          [editable proposal textarea]                 [Accept] [Re-run]
//          ↳ tiny metadata line (confidence, model)
function renderProposedRow(row, idx) {
    const saving = row.status === 'saving';
    const violations = row.glossaryViolations || [];
    const violationBanner = violations.length > 0
        ? `<div class="review-glossary-warn" title="The LLM's output did not use one of your glossary terms verbatim. Edit the text or re-run to try again.">
             <span class="review-glossary-warn-icon" aria-hidden="true">⚠</span>
             <span>
               Glossary not respected: ${violations.map(v =>
                   `<code>${escapeHtml(v.source_term)}</code> should be <code>${escapeHtml(v.target_term)}</code>`
               ).join(', ')}.
             </span>
           </div>`
        : '';
    const isReverse = row.sourceLang && row.sourceLang !== state.sourceLang;
    const dirBadge = isReverse
        ? `<span class="review-direction-badge" title="Back-filling ${escapeAttr(row.lang)} from the existing ${escapeAttr(row.sourceLang)} translation.">${escapeHtml(row.sourceLang)} → ${escapeHtml(row.lang)}</span>`
        : '';
    return `
        <div class="review-row review-row-proposed" data-idx="${idx}" data-id="${escapeAttr(row.id)}" ${violations.length > 0 ? 'data-glossary-warn="true"' : ''} ${isReverse ? 'data-reverse="true"' : ''}>
            <div class="review-key" title="${escapeAttr(row.ns)}.${escapeAttr(row.key)}">
                <span class="key-ns-tag">${escapeHtml(row.ns)}</span>
                <span class="review-key-name">${escapeHtml(row.key)}</span>
                ${dirBadge}
            </div>
            <div class="review-proposal">
                <div class="review-source review-source-plain" title="${escapeAttr(row.source)}">${escapeHtml(row.source)}</div>
                <textarea class="review-proposal-input" ${saving ? 'disabled' : ''} rows="1" data-action="edit-proposal">${escapeHtml(row.proposal)}</textarea>
                ${violationBanner}
                <div class="review-proposal-meta">
                    ${row.confidence !== null ? `<span class="review-meta-pill">${Math.round(row.confidence * 100)}% confidence</span>` : ''}
                    ${row.translatedBy ? `<span class="review-meta-pill">by ${escapeHtml(row.translatedBy)}</span>` : ''}
                </div>
            </div>
            <div class="review-actions review-actions-proposed">
                <button class="review-btn review-btn-accept" data-action="accept-row" ${saving ? 'disabled' : ''} title="Save this translation (Enter)">
                    ${saving ? '⏳' : '✓ Accept'}
                </button>
                <button class="review-btn review-btn-secondary" data-action="rerun-row" ${saving ? 'disabled' : ''} title="Discard this proposal and ask the LLM again">
                    ↻
                </button>
                <button class="review-btn review-btn-secondary" data-action="reject-row" ${saving ? 'disabled' : ''} title="Discard proposal (Esc)">
                    ✕
                </button>
            </div>
        </div>
    `;
}

function bindReviewGroupHeaders(container) {
    container.querySelectorAll('[data-action="toggle-group"]').forEach(el => {
        el.addEventListener('click', (e) => {
            if (e.target.closest('button')) return;
            const lang = el.dataset.lang;
            state.expanded[lang] = !state.expanded[lang];
            renderReview();
        });
    });
    container.querySelectorAll('[data-action="translate-lang"]').forEach(el => {
        el.addEventListener('click', (e) => { e.stopPropagation(); suggestLang(el.dataset.lang); });
    });
    container.querySelectorAll('[data-action="accept-lang"]').forEach(el => {
        el.addEventListener('click', (e) => { e.stopPropagation(); acceptLang(el.dataset.lang); });
    });
}

/** Wire row clicks, action buttons, and proposal textarea (call on a group body, virtual body, or a single `.review-row`). */
function bindReviewRowsIn(root) {
    const rowEls = root.matches('.review-row')
        ? [root]
        : Array.from(root.querySelectorAll('.review-row'));
    rowEls.forEach(el => {
        const idx = parseInt(el.dataset.idx, 10);
        el.addEventListener('click', (e) => {
            if (e.target.closest('textarea, button')) return;
            state.activeIdx = idx;
            syncActiveRow();
        });

        const map = {
            'suggest-row': () => suggestRowByIdx(idx),
            'write-row':   () => writeRowByIdx(idx),
            'accept-row':  () => acceptRowByIdx(idx),
            'reject-row':  () => rejectRowByIdx(idx),
            'rerun-row':   () => rerunRowByIdx(idx),
            'skip-row':    () => skipRowByIdx(idx),
        };
        el.querySelectorAll('[data-action]').forEach(btn => {
            const act = btn.dataset.action;
            if (!map[act]) return;
            btn.addEventListener('click', (e) => { e.stopPropagation(); map[act](); });
        });

        const ta = el.querySelector('[data-action="edit-proposal"]');
        if (ta) {
            autoresize(ta);
            ta.addEventListener('input', () => {
                const row = findRowByIdx(idx);
                if (row) {
                    row.proposal = ta.value;
                    if (row.glossaryViolations && row.glossaryViolations.length > 0) {
                        row.glossaryViolations = [];
                        const warn = el.querySelector('.review-glossary-warn');
                        if (warn) warn.remove();
                        el.removeAttribute('data-glossary-warn');
                    }
                }
                autoresize(ta);
            });
            ta.addEventListener('keydown', (e) => {
                if (e.key === 'Enter' && !e.shiftKey) {
                    e.preventDefault();
                    acceptRowByIdx(idx);
                } else if (e.key === 'Escape') {
                    e.preventDefault();
                    rejectRowByIdx(idx);
                }
            });
        }
    });
}

function updateSummary(visible) {
    const summary = document.getElementById('reviewSummary');
    const proposed = visible.filter(r => r.status === 'proposed').length;
    const langs = new Set(visible.map(r => r.lang));
    const bits = [`${visible.length} missing`];
    if (proposed > 0) bits.push(`${proposed} awaiting review`);
    bits.push(`${langs.size} language${langs.size === 1 ? '' : 's'}`);
    summary.classList.remove('hidden');
    summary.textContent = bits.join(' · ');
}

function updateAcceptAllBtn() {
    const btn = document.getElementById('acceptAllBtn');
    if (!btn) return;
    const n = proposedRows().length;
    if (n === 0) {
        btn.classList.add('hidden');
    } else {
        btn.classList.remove('hidden');
        // Label matches the "✨ Suggest visible" sibling: both operate on the
        // current filtered/searched view, not the entire unfiltered backlog.
        btn.textContent = `✓ Accept visible ${n} proposal${n === 1 ? '' : 's'}`;
    }
}

function syncActiveRow() {
    const visible = visibleRows();
    if (visible.length === 0) return;
    if (state.activeIdx >= visible.length) state.activeIdx = visible.length - 1;
    if (state.activeIdx < 0) state.activeIdx = 0;
    document.querySelectorAll('.review-row').forEach(el => {
        el.removeAttribute('data-active');
    });
    const activeRow = findRowByIdx(state.activeIdx);
    if (activeRow && !state.expanded[activeRow.lang]) {
        state.expanded[activeRow.lang] = true;
        renderReview();
        return;
    }
    const target = document.querySelector(`.review-row[data-idx="${state.activeIdx}"]`);
    if (target) {
        target.setAttribute('data-active', 'true');
        target.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    } else {
        ensureVirtualReviewRowVisible(state.activeIdx);
    }
}

// -------------------------------------------------------------------------
// Per-row actions
// -------------------------------------------------------------------------

function findRowByIdx(idx) {
    const v = visibleRows();
    return v[idx] || null;
}

async function suggestRowByIdx(idx) {
    const row = findRowByIdx(idx);
    if (row) await suggestRow(row);
}

function writeRowByIdx(idx) {
    const row = findRowByIdx(idx);
    if (row) writeRow(row);
}

/** Open the proposal editor with an empty textarea — no LLM call. */
function writeRow(row) {
    if (row.status === 'previewing' || row.status === 'saving') return;
    row.status = 'proposed';
    row.proposal = '';
    row.originalProposal = '';
    row.confidence = 1;
    row.translatedBy = 'human';
    row.glossaryViolations = [];
    renderReview();
    requestAnimationFrame(() => {
        const el = document.querySelector(`.review-row[data-id="${cssEscape(row.id)}"] textarea.review-proposal-input`);
        if (el) {
            el.focus();
            autoresize(el);
        }
    });
}

async function suggestRow(row) {
    if (!row.source) {
        Utils.showToast(`No source text for ${row.ns}.${row.key}`, 'error');
        return;
    }
    row.status = 'previewing';
    rerenderRow(row);
    try {
        // Reverse rows (missing source-lang) carry their own sourceLang +
        // source text, so we pass both: the backend treats `source_text` as
        // an override for the usual source-lookup. Forward rows fall through
        // to the same call with `state.sourceLang` and no text override.
        const r = await api.translateSingle(
            row.key, row.lang, row.sourceLang || state.sourceLang, row.ns,
            /*dryRun=*/true, row.source || null,
        );
        if (!r.ok) {
            Utils.showToast(`Translation failed: ${r.error}`, 'error');
            row.status = 'error';
            rerenderRow(row);
            return;
        }
        const data = r.data || {};
        row.proposal = data.translation ?? '';
        row.originalProposal = row.proposal;
        row.confidence = data.confidence ?? null;
        row.translatedBy = data.model_used ?? '';
        row.glossaryViolations = data.glossary_violations || [];
        row.status = 'proposed';
        renderReview();
    } catch (err) {
        console.error('suggest error:', err);
        row.status = 'error';
        rerenderRow(row);
        Utils.showToast('Translation error', 'error');
    }
}

async function acceptRowByIdx(idx) {
    const row = findRowByIdx(idx);
    if (row && row.status === 'proposed') await acceptRow(row);
}

async function acceptRow(row) {
    if (!row.proposal || !row.proposal.trim()) {
        Utils.showToast('Cannot accept an empty translation', 'error');
        return;
    }
    row.status = 'saving';
    rerenderRow(row);
    try {
        const meta = buildAcceptMeta(row);
        const r = await api.updateTranslation(row.lang, row.key, row.proposal, meta, row.ns);
        if (!r.ok) {
            Utils.showToast(`Save failed: ${r.error}`, 'error');
            row.status = 'proposed';
            rerenderRow(row);
            return;
        }
        state.rows = state.rows.filter(x => x !== row);
        renderReview();
    } catch (err) {
        console.error('accept error:', err);
        row.status = 'proposed';
        rerenderRow(row);
        Utils.showToast('Error saving translation', 'error');
    }
}

/** Metadata for a saved proposal: pure Write → human@1.0; LLM → model/edited. */
function buildAcceptMeta(row) {
    const isHuman = row.translatedBy === 'human';
    return {
        confidence: isHuman ? 1 : (row.confidence ?? 0),
        translated_by: isHuman
            ? 'human'
            : (row.proposal === row.originalProposal ? (row.translatedBy || 'llm') : 'human-edited'),
        timestamp: new Date().toISOString(),
    };
}

function rejectRowByIdx(idx) {
    const row = findRowByIdx(idx);
    if (!row) return;
    row.status = 'pending';
    row.proposal = '';
    row.originalProposal = '';
    row.confidence = null;
    row.translatedBy = '';
    renderReview();
}

async function rerunRowByIdx(idx) {
    const row = findRowByIdx(idx);
    if (!row) return;
    row.status = 'pending';
    row.proposal = '';
    row.originalProposal = '';
    row.confidence = null;
    row.translatedBy = '';
    await suggestRow(row);
}

function skipRowByIdx(idx) {
    const row = findRowByIdx(idx);
    if (!row) return;
    row.skipped = true;
    renderReview();
}

// -------------------------------------------------------------------------
// Bulk actions
// -------------------------------------------------------------------------

async function suggestLang(lang) {
    // Scope to visible rows so the action matches the "(N)" count we rendered
    // in the group header — otherwise a search filter makes the toolbar lie.
    const allPending = visibleRows().filter(r => r.lang === lang && r.status === 'pending');
    if (allPending.length === 0) {
        Utils.showToast(`No pending rows for "${lang}" in the current view — accept or reject existing proposals first.`, 'info');
        return;
    }
    const targets = allPending.slice(0, MAX_SUGGEST_PER_LANG);
    const skipped = allPending.length - targets.length;
    let msg = `Generate ${targets.length} proposal${targets.length === 1 ? '' : 's'} for "${lang}"? You'll review each before anything is saved.`;
    if (skipped > 0) {
        msg += `\n\n${skipped} more pending in this view are skipped this round (max ${MAX_SUGGEST_PER_LANG} per language). Run Suggest again after reviewing, or narrow filters.`;
    }
    if (!confirm(msg)) return;
    for (const r of targets) { r.status = 'previewing'; }
    renderReview();

    await Promise.all(targets.map(async row => {
        if (!row.source) { row.status = 'error'; return; }
        try {
            const r = await api.translateSingle(
                row.key, row.lang, row.sourceLang || state.sourceLang, row.ns,
                /*dryRun=*/true, row.source || null,
            );
            if (!r.ok) {
                row.status = 'error';
                return;
            }
            const data = r.data || {};
            row.proposal = data.translation ?? '';
            row.originalProposal = row.proposal;
            row.confidence = data.confidence ?? null;
            row.translatedBy = data.model_used ?? '';
            row.glossaryViolations = data.glossary_violations || [];
            row.status = 'proposed';
        } catch (err) {
            console.error('suggestLang per-row error:', err);
            row.status = 'error';
        }
    }));
    renderReview();
    const toast = skipped > 0
        ? `Generated ${targets.length} for ${lang} (${skipped} still pending — run Suggest again or filter).`
        : `Generated ${targets.length} proposal${targets.length === 1 ? '' : 's'} — review below`;
    Utils.showToast(toast, 'info');
}

async function suggestVisible() {
    const pending = visibleRows().filter(r => r.status === 'pending');
    if (pending.length === 0) {
        Utils.showToast('No pending rows in the current view — accept or reject existing proposals first.', 'info');
        return;
    }
    const byLang = new Map();
    for (const r of pending) {
        if (!byLang.has(r.lang)) byLang.set(r.lang, []);
        byLang.get(r.lang).push(r);
    }
    const targets = [];
    const skipParts = [];
    for (const [lg, rows] of byLang) {
        const take = rows.slice(0, MAX_SUGGEST_PER_LANG);
        targets.push(...take);
        const skip = rows.length - take.length;
        if (skip > 0) skipParts.push(`${lg}: ${skip} not run`);
    }
    if (targets.length === 0) return;

    let confirmMsg = `Generate ${targets.length} proposal${targets.length === 1 ? '' : 's'} (${byLang.size} language${byLang.size === 1 ? '' : 's'}, up to ${MAX_SUGGEST_PER_LANG} each)? You'll review each before anything is saved.`;
    if (skipParts.length > 0) {
        confirmMsg += `\n\nSkipped this round (cap per language): ${skipParts.join('; ')}. Run again after reviewing or narrow filters.`;
    }
    if (!confirm(confirmMsg)) return;

    const btn = document.getElementById('translateMissingBtn');
    btn.disabled = true;
    const originalText = btn.textContent;
    btn.textContent = `⏳ Generating ${targets.length}…`;
    for (const r of targets) { r.status = 'previewing'; }
    renderReview();

    try {
        await Promise.all(targets.map(async row => {
            if (!row.source) { row.status = 'error'; return; }
            try {
                const r = await api.translateSingle(
                    row.key, row.lang, row.sourceLang || state.sourceLang, row.ns,
                    /*dryRun=*/true, row.source || null,
                );
                if (!r.ok) { row.status = 'error'; return; }
                const data = r.data || {};
                row.proposal = data.translation ?? '';
                row.originalProposal = row.proposal;
                row.confidence = data.confidence ?? null;
                row.translatedBy = data.model_used ?? '';
                row.glossaryViolations = data.glossary_violations || [];
                row.status = 'proposed';
            } catch (err) {
                console.error('suggestVisible per-row:', err);
                row.status = 'error';
            }
        }));
        const tail = skipParts.length > 0
            ? ` Some keys skipped (max ${MAX_SUGGEST_PER_LANG} per language); run again or filter.`
            : '';
        Utils.showToast(`Generated ${targets.length} proposal${targets.length === 1 ? '' : 's'} — review below.${tail}`, 'info');
    } finally {
        btn.disabled = false;
        btn.textContent = originalText;
        renderReview();
    }
}

async function acceptLang(lang) {
    // Visible-row scoping keeps the group header's "(N proposed)" count
    // honest even when a search filter is active.
    const rows = visibleRows().filter(r => r.lang === lang && r.status === 'proposed' && r.proposal.trim());
    if (rows.length === 0) return;
    if (!confirm(`Accept ${rows.length} proposed translation${rows.length === 1 ? '' : 's'} for "${lang}"?`)) return;
    await acceptMany(rows);
}

async function acceptAll() {
    // proposedRows() is already scoped to visibleRows(), so this accepts
    // exactly what the toolbar button advertises — no ghost rows.
    const rows = proposedRows().filter(r => r.proposal.trim());
    if (rows.length === 0) return;
    if (!confirm(`Accept ${rows.length} proposed translation${rows.length === 1 ? '' : 's'} in the current view?`)) return;
    await acceptMany(rows);
}

async function acceptMany(rows) {
    for (const r of rows) { r.status = 'saving'; }
    renderReview();

    // PUTs run in parallel — the /api/translations/:lang/:key endpoint is
    // per-row, so parallelising saves a lot of wall-time when reviewing
    // dozens of proposals at once.
    const results = await Promise.all(rows.map(async row => {
        try {
            const meta = buildAcceptMeta(row);
            const r = await api.updateTranslation(row.lang, row.key, row.proposal, meta, row.ns);
            return { row, ok: r.ok, error: r.error };
        } catch (err) {
            return { row, ok: false, error: err };
        }
    }));

    let saved = 0;
    const failed = [];
    for (const res of results) {
        if (res.ok) {
            state.rows = state.rows.filter(x => x !== res.row);
            saved++;
        } else {
            res.row.status = 'proposed';
            failed.push(res);
        }
    }
    renderReview();
    if (saved > 0) Utils.showToast(`✓ Saved ${saved} translation${saved === 1 ? '' : 's'}`, 'success');
    if (failed.length > 0) Utils.showToast(`${failed.length} save${failed.length === 1 ? '' : 's'} failed — see console`, 'error');
}

// -------------------------------------------------------------------------
// Lean row-level rerender (used while a row is mid-transition to avoid a
// full re-render that would lose focus)
// -------------------------------------------------------------------------

function rerenderRow(row) {
    const el = document.querySelector(`.review-row[data-id="${cssEscape(row.id)}"]`);
    if (!el) {
        refreshReviewVirtualLang(row.lang);
        return;
    }
    const idx = parseInt(el.dataset.idx, 10);
    const tmp = document.createElement('div');
    tmp.innerHTML = renderRow(row, idx);
    const replacement = tmp.firstElementChild;
    el.replaceWith(replacement);
    bindReviewRowsIn(replacement);
}

function autoresize(ta) {
    if (!ta) return;
    ta.style.height = 'auto';
    ta.style.height = `${Math.max(ta.scrollHeight, 34)}px`;
}

// -------------------------------------------------------------------------
// Toolbar / keyboard
// -------------------------------------------------------------------------

function setupEventHandlers() {
    document.getElementById('targetLanguage').addEventListener('change', async (e) => {
        state.target = e.target.value;
        await loadRows();
    });
    document.getElementById('namespaceFilter').addEventListener('change', async (e) => {
        state.namespace = e.target.value;
        await loadRows();
    });
    const searchDebounced = Utils.debounce(() => renderReview(), 150);
    document.getElementById('searchInput').addEventListener('input', (e) => {
        state.search = e.target.value;
        state.activeIdx = 0;
        searchDebounced();
    });
    document.getElementById('translateMissingBtn').addEventListener('click', suggestVisible);
    const acceptAllBtn = document.getElementById('acceptAllBtn');
    if (acceptAllBtn) acceptAllBtn.addEventListener('click', acceptAll);
}

function setupKeyboardShortcuts() {
    document.addEventListener('keydown', (e) => {
        const isMeta = e.metaKey || e.ctrlKey;
        const target = e.target;
        const inField = target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.tagName === 'SELECT');

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
        if (inField) return;

        if (e.key === 'j' || e.key === 'ArrowDown') {
            e.preventDefault();
            state.activeIdx++;
            syncActiveRow();
        } else if (e.key === 'k' || e.key === 'ArrowUp') {
            e.preventDefault();
            state.activeIdx--;
            syncActiveRow();
        } else if (e.key === 'Enter') {
            // Enter is the primary affordance for both flow steps:
            //   pending  → Suggest
            //   proposed → Accept
            e.preventDefault();
            const row = findRowByIdx(state.activeIdx);
            if (!row) return;
            if (row.status === 'pending' || row.status === 'error') suggestRowByIdx(state.activeIdx);
            else if (row.status === 'proposed') acceptRowByIdx(state.activeIdx);
        } else if (e.key === 'w') {
            e.preventDefault();
            const row = findRowByIdx(state.activeIdx);
            if (!row) return;
            if (row.status === 'pending' || row.status === 'error') writeRowByIdx(state.activeIdx);
        } else if (e.key === 'x' || e.key === 'Escape') {
            e.preventDefault();
            const row = findRowByIdx(state.activeIdx);
            if (!row) return;
            if (row.status === 'proposed') rejectRowByIdx(state.activeIdx);
            else skipRowByIdx(state.activeIdx);
        }
    });
}

// Shared utilities — see utils.js for the canonical definition.
const escapeHtml = Utils.escapeHtml;
const escapeAttr = Utils.escapeHtml;
function cssEscape(s) {
    return (window.CSS && CSS.escape) ? CSS.escape(s) : String(s).replace(/[^a-zA-Z0-9_-]/g, '\\$&');
}
