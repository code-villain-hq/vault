// Review page: one language at a time; the lead follows its state.
//
// Lifecycle for a single row:
//   pending  → (Suggest) → previewing → (LLM returns) → proposed
//          ↘ (Write)  → proposed (empty field, translatedBy=human)
//     ├ Accept   → PUT /api/translations  → gone from the list
//     ├ Edit     → the reviewer tweaks the field → Accept
//     └ Reject   → back to pending (nothing saved)
//   A suggestion that fails leaves the row in 'error'; Suggest tries again.
//
// Each target language is a tab, and so is the source language while it has
// back-fill keys (missing in the source, present in a target). The open tab
// lists its proposals first, under "To accept", the ones that need a look
// (a glossary miss, low confidence) on top; its missing keys follow. While
// proposals wait, "Accept N" is the one primary action; otherwise Suggest is.
// Both act on the open tab only, and the bulk Accept skips proposals that
// need a look: those are accepted one by one.
//
// Backend contract: /api/translate/single accepts `dry_run: true`, which
// produces the same result payload without touching the DB.

const state = {
    sourceLang: 'en',
    targetLangs: [],
    namespaces: [],
    // The open language tab.
    tab: '',
    search: '',
    // rows[] { id, lang, ns, key, source, sourceLang, status, proposal,
    //         confidence, translatedBy, originalProposal, glossaryViolations,
    //         error, skipped }
    //   status: 'pending' | 'previewing' | 'proposed' | 'saving' | 'error'
    rows: [],
    // The row j/k moves to, by id; activeIdx is its place in the list, so the
    // next row takes over when it is accepted, rejected or skipped.
    activeId: null,
    activeIdx: 0,
    config: null,
    // lang → number of suggestions running from the lead's Suggest.
    busy: {},
    // True when part of the list failed to load: an empty list then is not "all done".
    loadFailed: false,
};

/** Below this confidence a model's proposal needs a look (Utils.getConfidenceColor agrees). */
const LOW_CONFIDENCE = 0.8;
/** Max dry-run translations per language in one Suggest (server-friendly). */
const MAX_SUGGEST_PER_LANG = 50;

// Windowed Missing list: with many missing keys only a slice is mounted (same
// idea as the Translations page). A missing row is one line of fixed height,
// so the window's offsets are exact.
const REVIEW_VIRT_THRESHOLD = 120;
const REVIEW_VIRT_OVERSCAN = 8;
/** .review-row-missing is 34px tall, plus the list's 2px gap. */
const MISSING_ROW_HEIGHT = 36;

/** @type {{ rows: object[], bodyEl: HTMLElement, start: number, end: number } | null} */
let missingVirt = null;
let missingVirtRafPending = false;

document.addEventListener('DOMContentLoaded', async () => {
    document.querySelector('.review-search').insertAdjacentHTML('afterbegin', icon('search'));
    setupEventHandlers();
    setupKeyboardShortcuts();
    await initReview();
});

async function initReview() {
    try {
        const [configRes, langRes, nsRes] = await Promise.all([
            api.getConfig(),
            api.getLanguages(),
            api.getNamespaces(),
        ]);
        if (!configRes.ok || !langRes.ok) {
            showLoadFailure();
            return;
        }
        state.config = configRes.data;
        state.sourceLang = state.config.user_preferences.source_language || 'en';

        const allLangs = (langRes.ok ? langRes.data : [])
            .map(l => l.code || l).filter(Boolean);
        state.targetLangs = allLangs.filter(c => c !== state.sourceLang).sort();
        state.namespaces = nsRes.ok ? (nsRes.data.namespaces || []) : [];

        await loadRows();

        // ?target=xx (the Dashboard's links) opens that language; otherwise
        // the one with proposals waiting, else the one with the most missing.
        const queryTarget = new URLSearchParams(window.location.search).get('target');
        const valid = [state.sourceLang, ...state.targetLangs];
        state.tab = queryTarget && valid.includes(queryTarget) ? queryTarget : defaultTab();
        renderReview();
    } catch (err) {
        console.error('Init error:', err);
        Utils.showToast('Error initializing', 'error');
    }
}

async function loadRows() {
    const container = document.getElementById('reviewContainer');
    Utils.showLoading(container, 'Loading missing translations…');

    // Forward rows: keys missing in each target. Reverse (back-fill) rows:
    // keys missing in the source language itself but present in a target.
    const namespaces = state.namespaces.length ? state.namespaces : ['translation'];
    const forwardTasks = [];
    const reverseTasks = [];
    for (const ns of namespaces) {
        reverseTasks.push({ lang: state.sourceLang, ns });
        for (const lang of state.targetLangs) forwardTasks.push({ lang, ns });
    }

    try {
        const [forwardResults, reverseResults] = await Promise.all([
            Promise.all(forwardTasks.map(t => api.getMissingTranslations(t.lang, state.sourceLang, t.ns))),
            Promise.all(reverseTasks.map(t => api.getMissingSource(t.lang, t.ns))),
        ]);

        state.loadFailed = [...forwardResults, ...reverseResults].some(r => !r.ok);
        if (state.loadFailed) Utils.showToast('Part of the review did not load. Reload to try again.', 'error');

        const rows = [];
        const add = (t, item, sourceLang) => rows.push({
            id: `${t.lang}|${t.ns}|${item.key}`,
            lang: t.lang,
            ns: t.ns,
            key: item.key,
            source: item.source || '',
            // Forward rows translate from the global source language; a
            // back-fill row from the target it was sampled from.
            sourceLang,
            status: 'pending',
            proposal: '',
            confidence: null,
            translatedBy: '',
            originalProposal: '',
            glossaryViolations: [],
            error: '',
            skipped: false,
        });
        forwardTasks.forEach((t, i) => {
            const r = forwardResults[i];
            if (r.ok) for (const item of (r.data || [])) add(t, item, state.sourceLang);
        });
        reverseTasks.forEach((t, i) => {
            const r = reverseResults[i];
            if (r.ok) for (const item of (r.data || [])) add(t, item, item.source_lang || '');
        });
        rows.sort((a, b) => {
            if (a.lang !== b.lang) return a.lang.localeCompare(b.lang);
            if (a.ns !== b.ns) return a.ns.localeCompare(b.ns);
            return a.key.localeCompare(b.key);
        });
        state.rows = rows;
        state.activeId = null;
        state.activeIdx = 0;
    } catch (err) {
        console.error('Load error:', err);
        state.loadFailed = true;
        Utils.showToast('Error loading missing translations', 'error');
    }
}

/** The server did not answer: say so in the lead, not "nothing to review". */
function showLoadFailure() {
    document.getElementById('leadTitle').textContent = 'Review did not load';
    document.getElementById('leadSub').textContent = 'The Glyph server did not answer. Reload the page to try again.';
    document.getElementById('reviewContainer').innerHTML = '';
}

// -------------------------------------------------------------------------
// What the open tab shows
// -------------------------------------------------------------------------

function isProposal(row) {
    return row.status === 'proposed' || row.status === 'saving';
}

/** A proposal that breaks the glossary or that the model was unsure of. */
function needsLook(row) {
    if (!isProposal(row)) return false;
    if ((row.glossaryViolations || []).length > 0) return true;
    return isLowConfidence(row);
}

function isLowConfidence(row) {
    return row.translatedBy !== 'human' && row.confidence !== null && row.confidence < LOW_CONFIDENCE;
}

function matchesSearch(row, q) {
    if (!q) return true;
    return `${row.ns}.${row.key}`.toLowerCase().includes(q)
        || row.source.toLowerCase().includes(q)
        || (row.proposal || '').toLowerCase().includes(q);
}

/** Rows of a language that pass the filter (any tab; used for the counts). */
function rowsForLang(lang) {
    const q = state.search.trim().toLowerCase();
    return state.rows.filter(r => r.lang === lang && !r.skipped && matchesSearch(r, q));
}

/**
 * The open tab's rows in the order they show: proposals first (the ones
 * that need a look on top), then the missing keys. j/k walk this order.
 */
function visibleRows() {
    const { proposals, missing } = tabSections();
    return [...proposals, ...missing];
}

function tabSections() {
    const rows = rowsForLang(state.tab);
    const flagged = [];
    const calm = [];
    const missing = [];
    for (const r of rows) {
        if (!isProposal(r)) missing.push(r);
        else if (needsLook(r)) flagged.push(r);
        else calm.push(r);
    }
    return { proposals: [...flagged, ...calm], missing };
}

/** The tabs: every target language, and the source while it has back-fill keys. */
function tabLangs() {
    const hasBackFill = state.rows.some(r => r.lang === state.sourceLang && !r.skipped);
    const langs = [...state.targetLangs];
    if (hasBackFill || state.tab === state.sourceLang) langs.unshift(state.sourceLang);
    return langs;
}

function defaultTab() {
    const langs = tabLangs();
    const waiting = langs.find(l => state.rows.some(r => r.lang === l && !r.skipped && isProposal(r)));
    if (waiting) return waiting;
    let best = langs[0] || state.sourceLang;
    let most = -1;
    for (const l of langs) {
        const n = state.rows.filter(r => r.lang === l && !r.skipped).length;
        if (n > most) { most = n; best = l; }
    }
    return best;
}

function findRowById(id) {
    return state.rows.find(r => r.id === id) || null;
}

// -------------------------------------------------------------------------
// Rendering
// -------------------------------------------------------------------------

/**
 * Re-draw the tabs, the lead and the open tab's rows. A proposal field that
 * has focus keeps it (and its caret) across the re-draw.
 */
function renderReview({ scrollActive = true } = {}) {
    const focused = document.activeElement;
    const focusId = focused && focused.matches && focused.matches('textarea.review-proposal-input')
        ? focused.closest('.review-row')?.dataset.id
        : null;
    const sel = focusId ? [focused.selectionStart, focused.selectionEnd] : null;

    teardownMissingVirt();
    renderTabs();
    updateLead();

    const container = document.getElementById('reviewContainer');
    const { proposals, missing } = tabSections();
    resolveActive([...proposals, ...missing]);

    // A label per section only when both show; one list needs no heading.
    const both = proposals.length > 0 && missing.length > 0;
    const useVirt = missing.length >= REVIEW_VIRT_THRESHOLD;
    const parts = [];
    if (proposals.length > 0) {
        parts.push(`
            <section class="review-section" aria-label="To accept">
                ${both ? `<p class="group-label">To accept · ${proposals.length}</p>` : ''}
                <div class="list">${proposals.map(renderRow).join('')}</div>
            </section>`);
    }
    if (missing.length > 0) {
        parts.push(`
            <section class="review-section" aria-label="Missing">
                ${both ? `<p class="group-label">Missing · ${missing.length}</p>` : ''}
                <div class="list">${useVirt
                    ? `<div class="review-virtual-body" style="height:${missing.length * MISSING_ROW_HEIGHT}px"></div>`
                    : missing.map(renderRow).join('')}</div>
            </section>`);
    }
    container.innerHTML = parts.join('');
    bindReviewRowsIn(container);

    if (useVirt) {
        missingVirt = {
            rows: missing,
            bodyEl: container.querySelector('.review-virtual-body'),
            start: -1,
            end: -1,
        };
        paintMissingWindow();
    }

    if (focusId) {
        const ta = container.querySelector(`.review-row[data-id="${cssEscape(focusId)}"] textarea.review-proposal-input`);
        if (ta) {
            ta.focus({ preventScroll: true });
            ta.setSelectionRange(sel[0], sel[1]);
        }
    }
    markActive(scrollActive && !focusId);
}

function renderTabs() {
    const bar = document.getElementById('reviewBar');
    const tabs = document.getElementById('langTabs');
    const langs = tabLangs();
    bar.hidden = langs.length === 0;
    tabs.innerHTML = langs.map(lang => {
        const n = rowsForLang(lang).length;
        const current = lang === state.tab;
        const title = lang === state.sourceLang
            ? `Back-fill: keys missing in ${langName(lang)} that another language has`
            : `Keys missing in ${langName(lang)}`;
        // Leave out an empty count.
        return `<button type="button" class="ui-tab" role="tab" data-lang="${escapeAttr(lang)}"
                    aria-selected="${current}" tabindex="${current ? 0 : -1}" title="${escapeAttr(title)}">
                    ${escapeHtml(langName(lang))}${n > 0 ? ` <span class="ui-tab-count">${n}</span>` : ''}
                </button>`;
    }).join('');
}

// The row's key trails its text, faint mono, as ns.key; a back-fill row says
// which language its text comes from.
function renderKey(row) {
    const isReverse = row.sourceLang && row.sourceLang !== state.sourceLang;
    const from = isReverse
        ? ` <span class="review-from" title="${escapeAttr(`${langName(state.sourceLang)} is missing; this ${langName(row.sourceLang)} text is translated into ${langName(row.lang)}.`)}">from ${escapeHtml(langName(row.sourceLang))}</span>`
        : '';
    return `<span class="review-key" title="${escapeAttr(row.ns)}.${escapeAttr(row.key)}">${escapeHtml(row.ns)}.${escapeHtml(row.key)}</span>${from}`;
}

// Keyboard hints: every row carries them, review.css shows the active row's.
function renderHint(kind) {
    const k = (key, label) => `<span><span class="ui-kbd">${key}</span> ${label}</span>`;
    const keys = kind === 'proposal'
        ? [k('↵', 'accept'), k('esc', 'reject'), `<span><span class="ui-kbd">j</span><span class="ui-kbd">k</span> move</span>`]
        : [k('↵', 'suggest'), k('w', 'write'), k('x', 'skip'), `<span><span class="ui-kbd">j</span><span class="ui-kbd">k</span> move</span>`];
    return `<span class="review-hint" aria-hidden="true">${keys.join('')}</span>`;
}

function renderRow(row) {
    return isProposal(row) ? renderProposedRow(row) : renderMissingRow(row);
}

// A missing key: one line, its text leading and its key trailing. Suggest,
// Write and Skip show on hover or on the active row; a failure stays in view.
function renderMissingRow(row) {
    const busy = row.status === 'previewing';
    const failed = row.status === 'error'
        ? `<span class="ui-status ui-status-danger" title="${escapeAttr(row.error || 'The suggestion failed')}">Suggest failed</span>`
        : '';
    return `
        <div class="review-row review-row-missing ui-row" data-state="${escapeAttr(row.status)}" data-id="${escapeAttr(row.id)}">
            <div class="review-line">
                <span class="review-text" title="${escapeAttr(row.source)}">${escapeHtml(row.source)}</span>
                ${renderKey(row)}
            </div>
            <div class="review-actions">
                ${failed}
                ${busy ? '' : renderHint('missing')}
                <div class="review-quiet">
                    <button type="button" class="ui-button" data-action="suggest-row" ${busy ? 'disabled' : ''} title="Draft a proposal (nothing is saved until you accept)">
                        ${busy ? `${icon('loader', 'review-spin')}Suggesting…` : (row.status === 'error' ? 'Try again' : 'Suggest')}
                    </button>
                    <button type="button" class="ui-button" data-action="write-row" ${busy ? 'disabled' : ''} title="Type the translation yourself (nothing is saved until you accept)">Write</button>
                    <button type="button" class="ui-button ui-icon-button" data-action="skip-row" ${busy ? 'disabled' : ''} aria-label="Skip" title="Hide until reload (x)">${icon('x')}</button>
                </div>
            </div>
        </div>`;
}

// A proposal: the source text with its key, the editable proposal with what
// needs a look under it, then Accept. Re-run and Reject wait for hover or the
// active row.
function renderProposedRow(row) {
    const saving = row.status === 'saving';
    const violations = row.glossaryViolations || [];
    const glossary = violations.length > 0
        ? `<div class="review-glossary-warn" title="The proposal does not use a glossary term. Edit the text or suggest again.">
             ${icon('alert')}
             <span>Glossary: ${violations.map(v =>
                 `<code class="code">${escapeHtml(v.source_term)}</code> should be <code class="code">${escapeHtml(v.target_term)}</code>`
             ).join(', ')}.</span>
           </div>`
        : '';
    const low = isLowConfidence(row)
        ? `<span class="ui-status ui-status-warn">Low confidence · ${Math.round(row.confidence * 100)}%</span>`
        : '';
    return `
        <div class="review-row review-row-proposed ui-row" data-state="${escapeAttr(row.status)}" data-id="${escapeAttr(row.id)}" ${violations.length > 0 ? 'data-glossary-warn="true"' : ''}>
            <div class="review-source">
                <div class="review-text-full">${escapeHtml(row.source)}</div>
                <div class="review-key-line">${renderKey(row)}</div>
            </div>
            <div class="review-proposal">
                <textarea class="ui-field review-proposal-input" ${saving ? 'disabled' : ''} rows="1" data-action="edit-proposal"
                    aria-label="Translation of ${escapeAttr(row.ns)}.${escapeAttr(row.key)}" title="${escapeAttr(proposalOrigin(row))}">${escapeHtml(row.proposal)}</textarea>
                ${glossary}
                <div class="review-proposal-meta">${low}${saving ? '' : renderHint('proposal')}</div>
            </div>
            <div class="review-actions">
                <div class="review-quiet">
                    <button type="button" class="ui-button ui-icon-button" data-action="rerun-row" ${saving ? 'disabled' : ''} aria-label="Suggest again" title="Discard this proposal and ask again">${icon('refresh')}</button>
                    <button type="button" class="ui-button ui-icon-button" data-action="reject-row" ${saving ? 'disabled' : ''} aria-label="Reject" title="Discard the proposal (esc)">${icon('x')}</button>
                </div>
                <button type="button" class="ui-button" data-action="accept-row" ${saving ? 'disabled' : ''} title="Save this translation (↵)">
                    ${saving ? `${icon('loader', 'review-spin')}Saving…` : `${icon('check')}Accept`}
                </button>
            </div>
        </div>`;
}

// Where a proposal came from, for the field's tooltip: a confident score is
// not shown in the row.
function proposalOrigin(row) {
    if (row.translatedBy === 'human') return 'Written by you';
    const parts = [];
    if (row.confidence !== null) parts.push(`${Math.round(row.confidence * 100)}% confidence`);
    if (row.translatedBy) parts.push(`by ${row.translatedBy}`);
    return parts.join(' · ');
}

/** Wire row clicks, action buttons and proposal fields: root is a row or holds rows. */
function bindReviewRowsIn(root) {
    const rowEls = root.matches('.review-row') ? [root] : root.querySelectorAll('.review-row');
    rowEls.forEach(el => {
        const id = el.dataset.id;
        const activate = () => { if (state.activeId !== id) setActive(id, false); };
        el.addEventListener('click', activate);
        el.addEventListener('focusin', activate);

        const map = {
            'suggest-row': () => suggestRow(findRowById(id)),
            'write-row':   () => writeRow(findRowById(id)),
            'accept-row':  () => acceptRow(findRowById(id)),
            'reject-row':  () => rejectRow(findRowById(id)),
            'rerun-row':   () => rerunRow(findRowById(id)),
            'skip-row':    () => skipRow(findRowById(id)),
        };
        el.querySelectorAll('button[data-action]').forEach(btn => {
            const act = map[btn.dataset.action];
            if (act) btn.addEventListener('click', (e) => { e.stopPropagation(); activate(); act(); });
        });

        const ta = el.querySelector('[data-action="edit-proposal"]');
        if (ta) {
            autoresize(ta);
            ta.addEventListener('input', () => {
                const row = findRowById(id);
                if (row) {
                    row.proposal = ta.value;
                    // An edit answers a glossary miss; the lead's count follows.
                    if (row.glossaryViolations && row.glossaryViolations.length > 0) {
                        row.glossaryViolations = [];
                        el.querySelector('.review-glossary-warn')?.remove();
                        el.removeAttribute('data-glossary-warn');
                        updateLead();
                    }
                }
                autoresize(ta);
            });
            ta.addEventListener('keydown', (e) => {
                if (e.key === 'Enter' && !e.shiftKey) {
                    e.preventDefault();
                    acceptRow(findRowById(id));
                } else if (e.key === 'Escape') {
                    e.preventDefault();
                    rejectRow(findRowById(id));
                }
            });
        }
    });
}

// -------------------------------------------------------------------------
// The lead: what the open tab needs, and its one primary action
// -------------------------------------------------------------------------

function updateLead() {
    const name = langName(state.tab);
    const { proposals, missing } = tabSections();
    const flagged = proposals.filter(needsLook).length;
    const acceptable = acceptableRows(proposals).length;
    const suggestable = missing.filter(r => r.status === 'pending' || r.status === 'error').length;
    const failed = missing.filter(r => r.status === 'error');
    const busy = state.busy[state.tab] || 0;
    const q = state.search.trim();
    const anyLeft = state.rows.some(r => !r.skipped);

    let title;
    let sub;
    if (!anyLeft && state.loadFailed) {
        title = 'Review did not load';
        sub = escapeHtml('The Glyph server did not answer. Reload the page to try again.');
    } else if (!anyLeft) {
        title = 'Nothing to review';
        sub = escapeHtml('Every translation is in.');
    } else if (proposals.length === 0 && missing.length === 0) {
        title = q ? `No ${name} keys match “${q}”` : `Nothing missing in ${name}`;
        sub = escapeHtml(q ? 'Clear the filter to see every key.' : `Every ${name} key is in.`);
    } else if (proposals.length > 0) {
        title = `${plural(proposals.length, `${name} suggestion`)} to accept`;
        const bits = [];
        if (missing.length > 0) bits.push(`${plural(missing.length, 'more key')} ${missing.length === 1 ? 'is' : 'are'} missing in ${name}.`);
        bits.push('Nothing is saved until you accept.');
        sub = escapeHtml(bits.join(' '));
    } else {
        title = `${plural(missing.length, 'key')} missing in ${name}`;
        sub = escapeHtml(state.tab === state.sourceLang
            ? `Their text comes from another language. Nothing is saved until you accept.`
            : 'Suggest drafts a translation for each. Nothing is saved until you accept.');
    }
    // A failed suggestion says why, and where to fix it.
    if (failed.length > 0 && busy === 0) {
        const why = failed[0].error || 'the suggestion failed';
        sub = escapeHtml(`Suggest failed for ${plural(failed.length, 'key')}: ${why}.`)
            + (why === MODEL_DOWN ? ' <a class="ui-link" href="/ui/settings.html">Start it in Settings</a>' : '');
    }
    document.getElementById('leadTitle').textContent = title;
    document.getElementById('leadSub').innerHTML = sub;

    // Accept N leads while proposals wait; then Suggest is secondary.
    const acceptBtn = document.getElementById('acceptBtn');
    acceptBtn.hidden = acceptable === 0;
    if (acceptable > 0) {
        acceptBtn.textContent = `Accept ${acceptable}`;
        acceptBtn.title = flagged > 0
            ? `Accept ${acceptable} — ${flagged} ${flagged === 1 ? 'needs' : 'need'} a look first`
            : `Save ${acceptable === 1 ? 'this proposal' : `these ${acceptable} proposals`} in ${name}`;
    }

    const suggestBtn = document.getElementById('suggestBtn');
    suggestBtn.classList.toggle('ui-button-primary', proposals.length === 0);
    if (busy > 0) {
        suggestBtn.hidden = false;
        suggestBtn.disabled = true;
        suggestBtn.innerHTML = `${icon('loader', 'review-spin')}Suggesting ${busy}…`;
        suggestBtn.title = '';
    } else {
        const n = Math.min(suggestable, MAX_SUGGEST_PER_LANG);
        suggestBtn.hidden = suggestable === 0;
        suggestBtn.disabled = false;
        suggestBtn.textContent = `Suggest ${n}`;
        suggestBtn.title = suggestable > MAX_SUGGEST_PER_LANG
            ? `Draft proposals for the first ${MAX_SUGGEST_PER_LANG} of ${suggestable} missing keys in ${name}; nothing is saved until you accept`
            : `Draft a proposal for each missing key in ${name}; nothing is saved until you accept`;
    }
}

/** Proposals the lead's Accept takes: not needing a look, and not empty. */
function acceptableRows(proposals) {
    return proposals.filter(r => r.status === 'proposed' && !needsLook(r) && r.proposal.trim());
}

function plural(n, word) {
    return `${n} ${word}${n === 1 ? '' : 's'}`;
}

function langName(code) {
    return Utils.langInfo(code).name;
}

// -------------------------------------------------------------------------
// The active row
// -------------------------------------------------------------------------

/** Keep activeId on a shown row: the same one if it is still here, else the one now at its place. */
function resolveActive(list) {
    if (list.length === 0) { state.activeId = null; state.activeIdx = 0; return; }
    const i = list.findIndex(r => r.id === state.activeId);
    if (i >= 0) { state.activeIdx = i; return; }
    state.activeIdx = Math.max(0, Math.min(state.activeIdx, list.length - 1));
    state.activeId = list[state.activeIdx].id;
}

function setActive(id, scroll) {
    state.activeId = id;
    resolveActive(visibleRows());
    markActive(scroll);
}

function moveActive(delta) {
    const list = visibleRows();
    if (list.length === 0) return;
    resolveActive(list);
    const i = Math.max(0, Math.min(state.activeIdx + delta, list.length - 1));
    setActive(list[i].id, true);
}

function markActive(scroll) {
    document.querySelectorAll('.review-row[data-active]').forEach(el => el.removeAttribute('data-active'));
    if (!state.activeId) return;
    if (scroll) ensureMissingRowMounted(state.activeId);
    const el = document.querySelector(`.review-row[data-id="${cssEscape(state.activeId)}"]`);
    if (!el) return;
    el.setAttribute('data-active', 'true');
    if (scroll) el.scrollIntoView({ block: 'nearest' });
}

// -------------------------------------------------------------------------
// Windowed Missing list
// -------------------------------------------------------------------------

function teardownMissingVirt() {
    missingVirt = null;
}

function paintMissingWindow() {
    const v = missingVirt;
    if (!v?.bodyEl?.isConnected) return;
    const rect = v.bodyEl.getBoundingClientRect();
    const top = Math.max(0, -rect.top);
    const n = v.rows.length;
    const start = Math.max(0, Math.floor(top / MISSING_ROW_HEIGHT) - REVIEW_VIRT_OVERSCAN);
    const end = Math.min(n, Math.ceil((top + window.innerHeight) / MISSING_ROW_HEIGHT) + REVIEW_VIRT_OVERSCAN);
    if (start === v.start && end === v.end) return;
    v.start = start;
    v.end = end;
    const parts = [];
    for (let i = start; i < end; i++) {
        parts.push(`<div class="v-review-wrap" style="transform:translateY(${i * MISSING_ROW_HEIGHT}px)">${renderMissingRow(v.rows[i])}</div>`);
    }
    v.bodyEl.innerHTML = parts.join('');
    bindReviewRowsIn(v.bodyEl);
    const el = state.activeId && v.bodyEl.querySelector(`.review-row[data-id="${cssEscape(state.activeId)}"]`);
    if (el) el.setAttribute('data-active', 'true');
}

/** Scroll a windowed missing row into the mounted slice before it is marked. */
function ensureMissingRowMounted(id) {
    const v = missingVirt;
    if (!v?.bodyEl) return;
    const i = v.rows.findIndex(r => r.id === id);
    if (i < 0 || (i >= v.start && i < v.end && document.querySelector(`.review-row[data-id="${cssEscape(id)}"]`))) return;
    const rowTop = window.scrollY + v.bodyEl.getBoundingClientRect().top + i * MISSING_ROW_HEIGHT;
    window.scrollTo({ top: Math.max(0, rowTop - window.innerHeight * 0.3) });
    paintMissingWindow();
}

function onWindowScroll() {
    if (!missingVirt || missingVirtRafPending) return;
    missingVirtRafPending = true;
    requestAnimationFrame(() => {
        missingVirtRafPending = false;
        paintMissingWindow();
    });
}

// -------------------------------------------------------------------------
// Per-row actions
// -------------------------------------------------------------------------

/** Re-draw one row in place, so a transition does not move focus or scroll. */
function rerenderRow(row) {
    const el = document.querySelector(`.review-row[data-id="${cssEscape(row.id)}"]`);
    if (!el) return;
    const tmp = document.createElement('div');
    tmp.innerHTML = renderRow(row);
    const replacement = tmp.firstElementChild;
    if (el.hasAttribute('data-active')) replacement.setAttribute('data-active', 'true');
    el.replaceWith(replacement);
    bindReviewRowsIn(replacement);
}

/** Open the proposal field empty, with no model call. */
function writeRow(row) {
    if (!row || row.status === 'previewing' || row.status === 'saving') return;
    row.status = 'proposed';
    row.proposal = '';
    row.originalProposal = '';
    row.confidence = 1;
    row.translatedBy = 'human';
    row.glossaryViolations = [];
    row.error = '';
    state.activeId = row.id;
    renderReview({ scrollActive: false });
    const el = document.querySelector(`.review-row[data-id="${cssEscape(row.id)}"] textarea.review-proposal-input`);
    if (el) {
        el.focus();
        autoresize(el);
    }
}

/** Ask for a dry-run proposal for one row; on success the row's fields fill in. */
async function draft(row) {
    if (!row.source) {
        row.status = 'error';
        row.error = 'it has no source text';
        return false;
    }
    try {
        // Back-fill rows carry their own source language and text; the
        // backend takes `source_text` over its own lookup.
        const r = await api.translateSingle(
            row.key, row.lang, row.sourceLang || state.sourceLang, row.ns,
            /*dryRun=*/true, row.source || null,
        );
        if (!r.ok) {
            row.status = 'error';
            row.error = friendlyError(r.error);
            return false;
        }
        const data = r.data || {};
        row.proposal = data.translation ?? '';
        row.originalProposal = row.proposal;
        row.confidence = data.confidence ?? null;
        row.translatedBy = data.model_used ?? '';
        row.glossaryViolations = data.glossary_violations || [];
        row.error = '';
        row.status = 'proposed';
        return true;
    } catch (err) {
        console.error('suggest error:', err);
        row.status = 'error';
        row.error = friendlyError(err && err.message);
        return false;
    }
}

async function suggestRow(row) {
    if (!row || row.status === 'previewing' || row.status === 'saving' || isProposal(row)) return;
    row.status = 'previewing';
    rerenderRow(row);
    const ok = await draft(row);
    if (!ok) Utils.showToast(failureToast(row.error), 'error');
    // The row moves up to To accept; if it is still the active one, the view
    // follows it, so ↵ then accepts what is on screen.
    renderReview({ scrollActive: state.activeId === row.id });
}

async function acceptRow(row) {
    if (!row || row.status !== 'proposed') return;
    if (!row.proposal || !row.proposal.trim()) {
        Utils.showToast('Cannot accept an empty translation', 'error');
        return;
    }
    row.status = 'saving';
    rerenderRow(row);
    try {
        const r = await api.updateTranslation(row.lang, row.key, row.proposal, buildAcceptMeta(row), row.ns);
        if (!r.ok) {
            Utils.showToast(`Save failed: ${r.error}`, 'error');
            row.status = 'proposed';
            rerenderRow(row);
            return;
        }
        leaveRow(row);
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

/** A row about to leave its place: the row now there becomes the active one. */
function leaveRow(row) {
    if (state.activeId !== row.id) return;
    const i = visibleRows().findIndex(r => r.id === row.id);
    if (i >= 0) state.activeIdx = i;
    state.activeId = null;
}

function clearProposal(row) {
    row.status = 'pending';
    row.proposal = '';
    row.originalProposal = '';
    row.confidence = null;
    row.translatedBy = '';
    row.glossaryViolations = [];
}

function rejectRow(row) {
    if (!row || row.status !== 'proposed') return;
    leaveRow(row);
    clearProposal(row);
    renderReview();
}

async function rerunRow(row) {
    if (!row || row.status !== 'proposed') return;
    clearProposal(row);
    renderReview({ scrollActive: false });
    await suggestRow(row);
}

function skipRow(row) {
    if (!row || isProposal(row) || row.status === 'previewing') return;
    leaveRow(row);
    row.skipped = true;
    renderReview();
}

// -------------------------------------------------------------------------
// The lead's actions: on the open tab only
// -------------------------------------------------------------------------

async function suggestTab() {
    const lang = state.tab;
    const name = langName(lang);
    if (state.busy[lang]) return;
    const all = tabSections().missing.filter(r => r.status === 'pending' || r.status === 'error');
    if (all.length === 0) return;
    const targets = all.slice(0, MAX_SUGGEST_PER_LANG);
    const left = all.length - targets.length;
    let msg = `Draft ${plural(targets.length, 'proposal')} for ${name}? You review each before anything is saved.`;
    if (left > 0) msg += `\n\nThe other ${left} wait for the next round (at most ${MAX_SUGGEST_PER_LANG} at a time).`;
    if (!confirm(msg)) return;

    state.busy[lang] = targets.length;
    for (const r of targets) { r.status = 'previewing'; r.error = ''; }
    renderReview({ scrollActive: false });

    let drafted = 0;
    let firstError = '';
    try {
        const results = await Promise.all(targets.map(draft));
        results.forEach((ok, i) => {
            if (ok) drafted++;
            else if (!firstError) firstError = targets[i].error;
        });
    } finally {
        delete state.busy[lang];
        renderReview({ scrollActive: false });
    }
    const failed = targets.length - drafted;
    if (drafted === 0) {
        Utils.showToast(failureToast(firstError || 'no proposals came back'), 'error');
    } else {
        const tail = failed > 0 ? ` ${failed} failed: ${firstError}.` : '';
        Utils.showToast(`Drafted ${plural(drafted, 'proposal')} for ${name}.${tail}`, failed > 0 ? 'error' : 'info');
    }
}

async function acceptTab() {
    const name = langName(state.tab);
    const { proposals } = tabSections();
    const rows = acceptableRows(proposals);
    if (rows.length === 0) return;
    const flagged = proposals.filter(needsLook).length;
    let msg = `Accept ${plural(rows.length, `${name} proposal`)}?`;
    if (flagged > 0) msg += `\n\n${flagged} that ${flagged === 1 ? 'needs' : 'need'} a look ${flagged === 1 ? 'stays' : 'stay'} in the list, to accept one by one.`;
    if (!confirm(msg)) return;
    await acceptMany(rows);
}

async function acceptMany(rows) {
    for (const r of rows) { r.status = 'saving'; }
    renderReview({ scrollActive: false });

    // The PUT is per row; in parallel they take a fraction of the time.
    const results = await Promise.all(rows.map(async row => {
        try {
            const r = await api.updateTranslation(row.lang, row.key, row.proposal, buildAcceptMeta(row), row.ns);
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
            console.error('accept failed:', res.row.id, res.error);
            res.row.status = 'proposed';
            failed.push(res);
        }
    }
    renderReview();
    if (saved > 0) Utils.showToast(`Saved ${plural(saved, 'translation')}`, 'success');
    if (failed.length > 0) Utils.showToast(`${failed.length} save${failed.length === 1 ? '' : 's'} failed — see console`, 'error');
}

// A model that is not running reads as such, not as a socket error.
const MODEL_DOWN = 'the translation model isn’t reachable';

/** A failure for a toast: a model that is down says where to start it. */
function failureToast(msg) {
    return `Suggest failed: ${msg}${msg === MODEL_DOWN ? '. Start it in Settings.' : ''}`;
}

function friendlyError(msg) {
    const m = String(msg || '');
    if (/failed to call LLM|connection refused|dial tcp|no such host|timeout|Network error|Failed to fetch/i.test(m)) {
        return MODEL_DOWN;
    }
    return m || 'the suggestion failed';
}

function autoresize(ta) {
    if (!ta) return;
    ta.style.height = 'auto';
    ta.style.height = `${Math.max(ta.scrollHeight, 34)}px`;
}

// -------------------------------------------------------------------------
// Tabs, filter, keyboard
// -------------------------------------------------------------------------

function openTab(lang, focusTab = false) {
    if (!lang || lang === state.tab) return;
    state.tab = lang;
    state.activeId = null;
    state.activeIdx = 0;
    const url = new URL(window.location.href);
    url.searchParams.set('target', lang);
    history.replaceState(null, '', url);
    renderReview({ scrollActive: false });
    if (focusTab) document.querySelector(`#langTabs .ui-tab[data-lang="${cssEscape(lang)}"]`)?.focus();
}

function setupEventHandlers() {
    const tabs = document.getElementById('langTabs');
    tabs.addEventListener('click', (e) => {
        const tab = e.target.closest('.ui-tab');
        if (tab) openTab(tab.dataset.lang);
    });
    // Arrow keys move between tabs, as in any tab strip.
    tabs.addEventListener('keydown', (e) => {
        if (e.key !== 'ArrowLeft' && e.key !== 'ArrowRight') return;
        const langs = tabLangs();
        const i = langs.indexOf(state.tab);
        const next = langs[(i + (e.key === 'ArrowRight' ? 1 : -1) + langs.length) % langs.length];
        e.preventDefault();
        openTab(next, true);
    });

    const searchDebounced = Utils.debounce(() => renderReview({ scrollActive: false }), 150);
    document.getElementById('searchInput').addEventListener('input', (e) => {
        state.search = e.target.value;
        state.activeId = null;
        state.activeIdx = 0;
        searchDebounced();
    });
    document.getElementById('suggestBtn').addEventListener('click', suggestTab);
    document.getElementById('acceptBtn').addEventListener('click', acceptTab);
    window.addEventListener('scroll', onWindowScroll, { passive: true });
    window.addEventListener('resize', onWindowScroll);
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
        if (inField || isMeta || e.altKey) return;

        if (e.key === 'j' || e.key === 'ArrowDown') {
            e.preventDefault();
            moveActive(1);
        } else if (e.key === 'k' || e.key === 'ArrowUp') {
            e.preventDefault();
            moveActive(-1);
        } else if (e.key === 'Enter') {
            // A focused button or link takes its own Enter.
            if (target && target.closest && target.closest('button, a')) return;
            // ↵ is the next step: suggest a missing key, accept a proposal.
            e.preventDefault();
            const row = findRowById(state.activeId);
            if (!row) return;
            if (row.status === 'pending' || row.status === 'error') suggestRow(row);
            else if (row.status === 'proposed') acceptRow(row);
        } else if (e.key === 'w') {
            e.preventDefault();
            const row = findRowById(state.activeId);
            if (row && (row.status === 'pending' || row.status === 'error')) writeRow(row);
        } else if (e.key === 'x' || e.key === 'Escape') {
            e.preventDefault();
            const row = findRowById(state.activeId);
            if (!row) return;
            if (row.status === 'proposed') rejectRow(row);
            else skipRow(row);
        }
    });
}

// Shared utilities — see utils.js for the canonical definition.
const escapeHtml = Utils.escapeHtml;
const escapeAttr = Utils.escapeHtml;
function cssEscape(s) {
    return (window.CSS && CSS.escape) ? CSS.escape(s) : String(s).replace(/[^a-zA-Z0-9_-]/g, '\\$&');
}
