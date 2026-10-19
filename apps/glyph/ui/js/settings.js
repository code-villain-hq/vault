// Settings page logic: general prefs + LLM list + language-pair matrix.

// Shared utilities — see utils.js for the canonical definition.
const escapeHtml = Utils.escapeHtml;
const escapeAttr = Utils.escapeHtml;

let currentConfig = null;
let allLanguages = [];
// Full `/api/languages` payload: `{code, translation_count, namespaces, is_source}`.
// Kept separately from `allLanguages` (codes only) because the Languages panel
// needs the counts/flags while the pair selects only care about the codes.
let languagesDetail = [];
// Cached `/api/namespaces/stats` rows for the Settings → Namespaces panel.
let settingsNsEntries = [];
let pairsState = {}; // local, editable copy keyed by "{src}_{tgt}"
let llmStatusTimer = null;
const LLM_STATUS_POLL_MS = 3000;

document.addEventListener('DOMContentLoaded', async () => {
    await initSettings();
    setupEventHandlers();
    setupKeyboardShortcuts();
});

async function initSettings() {
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
        languagesDetail = langResult.data || [];
        allLanguages = languagesDetail.map(l => l.code || l);

        pairsState = JSON.parse(JSON.stringify(currentConfig.language_pairs || {}));

        renderLLMModels();
        populateNewPairForm();
        renderPairs();
        renderLanguages();
        await refreshSettingsNamespaceList();

    } catch (error) {
        console.error('Error initializing settings:', error);
        Utils.showToast('Error initializing settings', 'error');
    }
}

function populateLanguageSelect(id, codes, excludeCode) {
    const sel = document.getElementById(id);
    if (!sel) return;
    sel.innerHTML = '';
    for (const code of codes) {
        if (excludeCode && code === excludeCode) continue;
        const opt = document.createElement('option');
        opt.value = code;
        opt.textContent = code;
        sel.appendChild(opt);
    }
}

function renderLLMModels() {
    const container = document.getElementById('llmModels');
    const models = currentConfig.llm.models || [];

    if (models.length === 0) {
        container.innerHTML = '<p class="text-slate-500 dark:text-slate-400 text-sm">No LLM models configured. Add them to <code>config.yaml</code> under <code>llm.models</code>.</p>';
        return;
    }

    container.innerHTML = models.map(model => `
        <div class="bg-slate-50 dark:bg-slate-900 rounded-lg p-4 border border-slate-200 dark:border-slate-700" data-llm-name="${escapeHtml(model.name)}">
            <div class="flex items-start justify-between gap-4 mb-3">
                <div class="grid grid-cols-1 sm:grid-cols-3 gap-4 flex-1 min-w-0">
                    <div>
                        <label class="block text-xs font-medium text-slate-600 dark:text-slate-400 mb-1">Name</label>
                        <p class="text-sm font-mono text-slate-900 dark:text-white">${escapeHtml(model.name)}</p>
                    </div>
                    <div>
                        <label class="block text-xs font-medium text-slate-600 dark:text-slate-400 mb-1">Endpoint</label>
                        <p class="text-sm font-mono text-slate-900 dark:text-white truncate">${escapeHtml(model.endpoint)}</p>
                    </div>
                    <div>
                        <label class="block text-xs font-medium text-slate-600 dark:text-slate-400 mb-1">Model</label>
                        <p class="text-sm font-mono text-slate-900 dark:text-white">${escapeHtml(model.model)}</p>
                    </div>
                </div>
                <div class="shrink-0 text-right">
                    <label class="block text-xs font-medium text-slate-600 dark:text-slate-400 mb-1">Status</label>
                    <div class="inline-flex items-center gap-2 px-2 py-1 rounded-md bg-white dark:bg-slate-800 border border-slate-200 dark:border-slate-700" data-llm-status-pill>
                        <span class="w-2 h-2 rounded-full bg-slate-400" data-llm-dot></span>
                        <span class="text-xs font-medium text-slate-600 dark:text-slate-300" data-llm-label>Checking…</span>
                    </div>
                </div>
            </div>
            <div class="grid grid-cols-1 sm:grid-cols-3 gap-2">
                <button class="llm-start-btn px-3 py-2 rounded text-sm font-medium bg-emerald-100 dark:bg-emerald-900/30 text-emerald-700 dark:text-emerald-400 hover:bg-emerald-200 dark:hover:bg-emerald-900/50 disabled:opacity-50 disabled:cursor-not-allowed transition-colors duration-200" data-name="${escapeHtml(model.name)}" disabled>
                    ▶ Start
                </button>
                <button class="llm-stop-btn px-3 py-2 rounded text-sm font-medium bg-rose-100 dark:bg-rose-900/30 text-rose-700 dark:text-rose-400 hover:bg-rose-200 dark:hover:bg-rose-900/50 disabled:opacity-50 disabled:cursor-not-allowed transition-colors duration-200" data-name="${escapeHtml(model.name)}" disabled>
                    ■ Stop
                </button>
                <button class="test-llm-btn px-3 py-2 rounded text-sm font-medium bg-indigo-100 dark:bg-indigo-900/30 text-indigo-600 dark:text-indigo-400 hover:bg-indigo-200 dark:hover:bg-indigo-900/50 transition-colors duration-200" data-endpoint="${escapeHtml(model.endpoint)}">
                    🧪 Test
                </button>
            </div>
            <p class="hidden mt-2 text-xs text-slate-500 dark:text-slate-400" data-llm-hint></p>
            <details class="hidden mt-2 group" data-llm-log-details>
                <summary class="cursor-pointer select-none text-xs font-medium text-slate-600 dark:text-slate-400 hover:text-slate-900 dark:hover:text-white inline-flex items-center gap-1">
                    <span class="transition-transform group-open:rotate-90">▸</span>
                    <span data-llm-log-summary>Console output</span>
                </summary>
                <pre class="mt-2 max-h-64 overflow-auto rounded-md bg-slate-900 text-slate-100 dark:bg-black text-[11px] leading-snug font-mono p-3 whitespace-pre-wrap break-words" data-llm-log></pre>
            </details>
        </div>
    `).join('');

    container.querySelectorAll('.test-llm-btn').forEach(btn => {
        btn.addEventListener('click', () => testLLMConnection(btn));
    });
    container.querySelectorAll('.llm-start-btn').forEach(btn => {
        btn.addEventListener('click', () => startLLMServer(btn));
    });
    container.querySelectorAll('.llm-stop-btn').forEach(btn => {
        btn.addEventListener('click', () => stopLLMServer(btn));
    });

    startLLMStatusPolling();
}

// Poll status for each configured LLM on an interval so the dot updates
// without the user needing to reload the page.
function startLLMStatusPolling() {
    if (llmStatusTimer) clearInterval(llmStatusTimer);
    refreshAllLLMStatuses();
    llmStatusTimer = setInterval(refreshAllLLMStatuses, LLM_STATUS_POLL_MS);
}

async function refreshAllLLMStatuses() {
    const models = currentConfig?.llm?.models || [];
    await Promise.all(models.map(m => refreshLLMStatus(m.name)));
}

async function refreshLLMStatus(name) {
    const card = document.querySelector(`[data-llm-name="${CSS.escape(name)}"]`);
    if (!card) return;

    const result = await api.getLLMStatus(name);
    if (!result.ok) {
        applyLLMStatusUI(card, {
            state: 'unknown',
            label: 'Unknown',
            hint: result.error,
            recentOutput: [],
            exited: false,
        });
        return;
    }
    const s = result.data;

    let state, label, hint;
    if (s.reachable && s.managed) {
        state = 'running-managed';
        label = `Running (pid ${s.pid})`;
        hint = '';
    } else if (s.reachable) {
        state = 'running-external';
        label = 'Running (external)';
        hint = 'This server wasn\'t started by the UI, so Stop won\'t work.';
    } else if (s.managed && s.exited) {
        // The child was ours and has died. Promote this to a dedicated state
        // so the dot/hint/log copy can all reflect "crashed" instead of the
        // optimistic "still starting…" it used to say.
        state = 'crashed';
        label = 'Exited';
        hint = s.exit_error
            ? `Process exited: ${s.exit_error}. Expand the console output for the full log.`
            : 'Process exited before it could serve requests. Expand the console output for details.';
    } else if (s.managed) {
        state = 'starting';
        label = 'Starting…';
        hint = 'Waiting for /health to respond.';
    } else {
        state = 'stopped';
        label = 'Stopped';
        hint = s.can_start ? '' : 'Set model_path or hf_repo+hf_file in config.yaml to enable Start.';
    }

    applyLLMStatusUI(card, {
        state,
        label,
        hint,
        canStart: s.can_start,
        reachable: s.reachable,
        managed: s.managed,
        exited: !!s.exited,
        exitError: s.exit_error || '',
        recentOutput: Array.isArray(s.recent_output) ? s.recent_output : [],
    });
}

function applyLLMStatusUI(card, { state, label, hint, canStart, reachable, managed, exited, exitError, recentOutput }) {
    const dot = card.querySelector('[data-llm-dot]');
    const labelEl = card.querySelector('[data-llm-label]');
    const hintEl = card.querySelector('[data-llm-hint]');
    const startBtn = card.querySelector('.llm-start-btn');
    const stopBtn = card.querySelector('.llm-stop-btn');

    const dotColors = {
        'running-managed': 'bg-emerald-500 animate-pulse',
        'running-external': 'bg-emerald-500',
        'starting': 'bg-amber-500 animate-pulse',
        'crashed': 'bg-rose-500',
        'stopped': 'bg-slate-400',
        'unknown': 'bg-slate-400',
    };
    dot.className = `w-2 h-2 rounded-full ${dotColors[state] || 'bg-slate-400'}`;
    labelEl.textContent = label;

    if (hint) {
        hintEl.textContent = hint;
        hintEl.classList.remove('hidden');
    } else {
        hintEl.classList.add('hidden');
    }

    // Start is only meaningful when the server is not reachable and the
    // config knows which GGUF to load. Stop is only meaningful when we're
    // the ones managing the process.
    startBtn.disabled = reachable || !canStart;
    stopBtn.disabled = !managed;

    updateLLMLogPanel(card, { state, recentOutput: recentOutput || [], exitError: exitError || '', exited: !!exited });
}

// updateLLMLogPanel renders the `<details>` console-output panel.
//
// Visibility rule: the panel is only shown when we actually have log lines
// *or* an exit_error to display — no point in a collapsible that expands to
// reveal nothing.
//
// Auto-expand rule: when the process has exited (state === 'crashed') and
// the user hasn't manually interacted with the panel yet (data-user-toggled),
// we open it once so the error is unmissable. After that we respect whatever
// the user picked — no forced reopening on every poll tick.
function updateLLMLogPanel(card, { state, recentOutput, exitError, exited }) {
    const details = card.querySelector('[data-llm-log-details]');
    const summary = card.querySelector('[data-llm-log-summary]');
    const pre = card.querySelector('[data-llm-log]');
    if (!details || !pre || !summary) return;

    const lines = [...recentOutput];
    if (exitError) {
        // Tag the exit status onto the end so the user sees the exact cause
        // right next to the final log lines (llama-server typically prints
        // the real reason just above the exit).
        lines.push('');
        lines.push(`[process exited: ${exitError}]`);
    }

    if (lines.length === 0) {
        details.classList.add('hidden');
        details.removeAttribute('open');
        // Reset the user-toggle memory so a *new* start attempt can auto-
        // expand the next crash again.
        details.removeAttribute('data-user-toggled');
        return;
    }

    details.classList.remove('hidden');

    // Only re-render the log body when the content actually changes — keeps
    // the scroll position stable when the user is reading.
    const joined = lines.join('\n');
    if (pre.textContent !== joined) {
        const nearBottom = (pre.scrollHeight - pre.scrollTop - pre.clientHeight) < 16;
        pre.textContent = joined;
        if (nearBottom) pre.scrollTop = pre.scrollHeight;
    }

    const lineCount = recentOutput.length;
    const summaryText = state === 'crashed'
        ? `Console output (${lineCount} line${lineCount === 1 ? '' : 's'}) — crash details`
        : `Console output (${lineCount} line${lineCount === 1 ? '' : 's'})`;
    summary.textContent = summaryText;

    // Wire up the "user toggled it" memo exactly once. We use `onclick` (not
    // `addEventListener`) so repeated renders don't stack listeners.
    if (!details.__userToggleWired) {
        details.__userToggleWired = true;
        details.addEventListener('toggle', () => {
            details.setAttribute('data-user-toggled', '1');
        });
    }

    // Auto-expand on a fresh crash the user hasn't acknowledged yet.
    if (state === 'crashed' && exited && !details.hasAttribute('data-user-toggled') && !details.open) {
        details.open = true;
        // Opening the panel via `open=true` fires a `toggle` event, which
        // our listener above will mark as user-toggled. That's wrong — this
        // is an *auto* expand. Clear the flag on the next tick so a later
        // manual close by the user is still respected.
        setTimeout(() => details.removeAttribute('data-user-toggled'), 0);
    }
}

async function startLLMServer(btn) {
    const name = btn.dataset.name;
    const card = btn.closest('[data-llm-name]');
    btn.disabled = true;
    const original = btn.innerHTML;
    btn.innerHTML = '⏳ Starting…';
    try {
        const result = await api.startLLM(name);
        if (!result.ok) {
            Utils.showToast(`Failed to start ${name}: ${result.error}`, 'error');
            btn.innerHTML = original;
            return;
        }
        Utils.showToast(`Starting ${name}…`, 'success');
        // The server needs a few seconds to bind the port; poll more
        // aggressively for a little bit so the user sees the transition.
        for (let i = 0; i < 5; i++) {
            await new Promise(r => setTimeout(r, 1000));
            await refreshLLMStatus(name);
        }
    } catch (err) {
        console.error('Error starting LLM:', err);
        Utils.showToast('Error starting LLM server', 'error');
    } finally {
        btn.innerHTML = original;
        refreshLLMStatus(name);
    }
}

async function stopLLMServer(btn) {
    const name = btn.dataset.name;
    btn.disabled = true;
    const original = btn.innerHTML;
    btn.innerHTML = '⏳ Stopping…';
    try {
        const result = await api.stopLLM(name);
        if (!result.ok) {
            Utils.showToast(`Failed to stop ${name}: ${result.error}`, 'error');
            return;
        }
        Utils.showToast(`Stopped ${name}`, 'success');
        await refreshLLMStatus(name);
    } catch (err) {
        console.error('Error stopping LLM:', err);
        Utils.showToast('Error stopping LLM server', 'error');
    } finally {
        btn.innerHTML = original;
        refreshLLMStatus(name);
    }
}

function populateNewPairForm() {
    populateLanguageSelect('newPairSource', allLanguages);
    populateLanguageSelect('newPairTarget', allLanguages);
    const modelSel = document.getElementById('newPairModel');
    modelSel.innerHTML = '';
    for (const m of currentConfig.llm.models || []) {
        const opt = document.createElement('option');
        opt.value = m.name;
        opt.textContent = m.name;
        modelSel.appendChild(opt);
    }
    // Prefer en→sv as the initial seed to avoid picking identical source/target.
    if (allLanguages.includes('en')) document.getElementById('newPairSource').value = 'en';
    const firstNonEn = allLanguages.find(c => c !== 'en');
    if (firstNonEn) document.getElementById('newPairTarget').value = firstNonEn;
}

function renderPairs() {
    const container = document.getElementById('pairsList');
    const keys = Object.keys(pairsState).sort();
    const models = currentConfig.llm.models || [];
    const modelNames = models.map(m => m.name);

    if (keys.length === 0) {
        container.innerHTML = '<p class="text-slate-500 dark:text-slate-400 text-sm italic">No language pairs configured yet. Add one below.</p>';
        return;
    }

    container.innerHTML = `
        <div class="hidden sm:grid grid-cols-12 gap-3 px-4 text-xs font-semibold uppercase tracking-wide text-slate-500 dark:text-slate-400">
            <div class="col-span-2">Direction</div>
            <div class="col-span-3">LLM</div>
            <div class="col-span-3">Temperature</div>
            <div class="col-span-3">Confidence ≥</div>
            <div class="col-span-1"></div>
        </div>
    ` + keys.map(key => renderPairRow(key, modelNames)).join('');

    for (const key of keys) {
        bindPairRow(key);
    }
}

function renderPairRow(key, modelNames) {
    const pair = pairsState[key] || {};
    const [src, tgt] = key.split('_');
    const temp = pair.temperature ?? 0.2;
    const threshold = pair.confidence_threshold ?? 0.7;
    const selectedModel = pair.model || modelNames[0] || '';
    const modelOptions = modelNames.length
        ? modelNames.map(n => `<option value="${escapeHtml(n)}" ${n === selectedModel ? 'selected' : ''}>${escapeHtml(n)}</option>`).join('')
        : '<option value="">(no models)</option>';
    const unknownModel = selectedModel && !modelNames.includes(selectedModel)
        ? `<option value="${escapeHtml(selectedModel)}" selected>${escapeHtml(selectedModel)} (missing)</option>`
        : '';

    return `
        <div class="grid grid-cols-1 sm:grid-cols-12 gap-3 items-center bg-slate-50 dark:bg-slate-900 rounded-lg p-4 border border-slate-200 dark:border-slate-700" data-pair-key="${escapeHtml(key)}">
            <div class="sm:col-span-2 font-mono text-sm text-slate-900 dark:text-white">
                <span class="inline-block px-2 py-1 bg-white dark:bg-slate-800 rounded border border-slate-200 dark:border-slate-700">${escapeHtml(src)} → ${escapeHtml(tgt)}</span>
            </div>
            <div class="sm:col-span-3">
                <select data-field="model" class="w-full px-3 py-2 rounded-lg border border-slate-300 dark:border-slate-600 dark:bg-slate-700 dark:text-white text-sm">
                    ${unknownModel}${modelOptions}
                </select>
            </div>
            <div class="sm:col-span-3 flex items-center gap-3">
                <input data-field="temperature" type="range" min="0" max="2" step="0.1" value="${temp}" class="flex-1 h-2 bg-slate-300 dark:bg-slate-600 rounded-lg appearance-none cursor-pointer accent-indigo-600">
                <span data-display="temperature" class="text-sm font-semibold text-slate-700 dark:text-slate-300 min-w-[2.5rem] text-right">${Number(temp).toFixed(1)}</span>
            </div>
            <div class="sm:col-span-3 flex items-center gap-3">
                <input data-field="confidence_threshold" type="range" min="0" max="1" step="0.05" value="${threshold}" class="flex-1 h-2 bg-slate-300 dark:bg-slate-600 rounded-lg appearance-none cursor-pointer accent-emerald-600">
                <span data-display="confidence_threshold" class="text-sm font-semibold text-slate-700 dark:text-slate-300 min-w-[2.5rem] text-right">${Number(threshold).toFixed(2)}</span>
            </div>
            <div class="sm:col-span-1 flex justify-end">
                <button data-action="delete" class="px-2 py-1 text-sm text-rose-600 hover:bg-rose-100 dark:hover:bg-rose-900/30 rounded transition-colors" title="Remove pair">✕</button>
            </div>
        </div>
    `;
}

function bindPairRow(key) {
    const row = document.querySelector(`[data-pair-key="${CSS.escape(key)}"]`);
    if (!row) return;

    row.querySelector('[data-field="model"]').addEventListener('change', (e) => {
        pairsState[key] = { ...pairsState[key], model: e.target.value };
    });

    const tempInput = row.querySelector('[data-field="temperature"]');
    const tempDisp = row.querySelector('[data-display="temperature"]');
    tempInput.addEventListener('input', (e) => {
        const v = parseFloat(e.target.value);
        pairsState[key] = { ...pairsState[key], temperature: v };
        tempDisp.textContent = v.toFixed(1);
    });

    const threshInput = row.querySelector('[data-field="confidence_threshold"]');
    const threshDisp = row.querySelector('[data-display="confidence_threshold"]');
    threshInput.addEventListener('input', (e) => {
        const v = parseFloat(e.target.value);
        pairsState[key] = { ...pairsState[key], confidence_threshold: v };
        threshDisp.textContent = v.toFixed(2);
    });

    row.querySelector('[data-action="delete"]').addEventListener('click', () => {
        delete pairsState[key];
        renderPairs();
    });
}

async function testLLMConnection(btn) {
    const endpoint = btn.dataset.endpoint;
    btn.disabled = true;
    const original = btn.textContent;
    btn.textContent = '⏳ Testing…';

    try {
        const result = await api.testLLM(endpoint);
        if (!result.ok) {
            Utils.showToast(`❌ Connection failed: ${result.error}`, 'error');
            return;
        }

        if (result.data.reachable) {
            Utils.showToast(`✓ Connected to ${endpoint}`, 'success');
        } else {
            Utils.showToast(`❌ Endpoint unreachable`, 'error');
        }
    } catch (error) {
        console.error('Error testing LLM:', error);
        Utils.showToast('Error testing connection', 'error');
    } finally {
        btn.textContent = original;
        btn.disabled = false;
    }
}

// ---------------------------------------------------------------------------
// Languages panel: list + add + per-row remove.
// ---------------------------------------------------------------------------

function renderLanguages() {
    const container = document.getElementById('languagesList');
    if (!container) return;

    if (!languagesDetail.length) {
        container.innerHTML = '<p class="text-slate-500 dark:text-slate-400 text-sm italic">No languages yet. Add one below.</p>';
        return;
    }

    // The source language always shows first (even if its code doesn't sort
    // first) — it's the pivot for every pair and can't be removed, so users
    // expect it at the top.
    const sorted = [...languagesDetail].sort((a, b) => {
        if (a.is_source && !b.is_source) return -1;
        if (b.is_source && !a.is_source) return 1;
        return a.code.localeCompare(b.code);
    });

    container.innerHTML = sorted.map(renderLanguageRow).join('');

    for (const btn of container.querySelectorAll('[data-action="remove-lang"]')) {
        btn.addEventListener('click', () => removeLanguage(btn.dataset.lang));
    }
}

function renderLanguageRow(lang) {
    const count = lang.translation_count || 0;
    const namespaces = (lang.namespaces || []).length;
    const isSource = !!lang.is_source;

    const sourceBadge = isSource
        ? '<span class="px-2 py-0.5 rounded-full text-[11px] font-semibold bg-indigo-100 text-indigo-700 dark:bg-indigo-900/40 dark:text-indigo-300">source</span>'
        : '';

    const countLabel = count === 0
        ? '<span class="text-xs text-slate-400 dark:text-slate-500 italic">no translations yet</span>'
        : `<span class="text-xs text-slate-500 dark:text-slate-400">${count.toLocaleString()} translations · ${namespaces} namespace${namespaces === 1 ? '' : 's'}</span>`;

    const removeBtn = isSource
        ? `<span class="text-xs text-slate-400 dark:text-slate-500" title="The source language is set in General. Change it there before removing.">locked</span>`
        : `<button data-action="remove-lang" data-lang="${escapeHtml(lang.code)}" class="px-3 py-1.5 text-sm text-rose-600 hover:bg-rose-100 dark:hover:bg-rose-900/30 rounded-lg transition-colors" title="Remove language">Remove</button>`;

    return `
        <div class="flex items-center justify-between gap-4 bg-slate-50 dark:bg-slate-900 rounded-lg p-3 border border-slate-200 dark:border-slate-700">
            <div class="flex items-center gap-3 min-w-0">
                <span class="font-mono text-sm font-semibold text-slate-900 dark:text-white">${escapeHtml(lang.code)}</span>
                ${sourceBadge}
                ${countLabel}
            </div>
            <div>${removeBtn}</div>
        </div>
    `;
}

async function addLanguage() {
    const input = document.getElementById('newLangCode');
    const btn = document.getElementById('addLangBtn');
    if (!input || !btn) return;

    const code = (input.value || '').trim().toLowerCase();
    if (!code) {
        Utils.showToast('Enter a language code', 'error');
        input.focus();
        return;
    }
    // Matches the server-side validator so the user sees the same rules without
    // a round-trip. Keep these in sync with `isValidLangCode` in handlers.go.
    if (!/^[a-z0-9][a-z0-9_-]{0,14}$/.test(code)) {
        Utils.showToast('Invalid code — use letters, digits, "-" or "_"', 'error');
        input.focus();
        return;
    }
    if (code === currentConfig?.user_preferences?.source_language) {
        Utils.showToast(`"${code}" is already the source language`, 'error');
        return;
    }
    if (languagesDetail.some(l => l.code === code)) {
        Utils.showToast(`"${code}" already exists`, 'error');
        return;
    }

    const original = btn.textContent;
    btn.disabled = true;
    btn.textContent = '⏳ Adding…';

    try {
        const result = await api.addLanguage(code);
        if (!result.ok) {
            Utils.showToast(`Failed to add language: ${result.error}`, 'error');
            return;
        }
        Utils.showToast(`✓ Added "${code}". Start translating from the Translations page.`, 'success');
        input.value = '';
        await refreshAfterLanguageChange();
    } catch (error) {
        console.error('Error adding language:', error);
        Utils.showToast('Error adding language', 'error');
    } finally {
        btn.disabled = false;
        btn.textContent = original;
    }
}

async function removeLanguage(code) {
    if (!code) return;
    const detail = languagesDetail.find(l => l.code === code);
    const count = detail?.translation_count || 0;
    const msg = count > 0
        ? `Remove "${code}"? This will permanently delete ${count.toLocaleString()} translations, all glossary entries for this language and any matching language pairs. There is no undo.`
        : `Remove "${code}"? Matching language pairs and glossary entries will also be cleared.`;
    if (!confirm(msg)) return;

    try {
        const result = await api.deleteLanguage(code);
        if (!result.ok) {
            Utils.showToast(`Failed to remove: ${result.error}`, 'error');
            return;
        }
        const parts = [`✓ Removed "${code}"`];
        if (result.data.pairs_removed) parts.push(`${result.data.pairs_removed} pair(s)`);
        if (result.data.glossary_removed) parts.push(`${result.data.glossary_removed} glossary entries`);
        Utils.showToast(parts.join(' · '), 'success');
        await refreshAfterLanguageChange();
    } catch (error) {
        console.error('Error removing language:', error);
        Utils.showToast('Error removing language', 'error');
    }
}

// refreshAfterLanguageChange re-fetches the config + languages so the UI
// reflects the new set everywhere: the Languages list, the default target
// dropdown, and the language_pairs table all depend on this data.
async function refreshAfterLanguageChange() {
    const [cfgRes, langRes] = await Promise.all([api.getConfig(), api.getLanguages()]);
    if (cfgRes.ok) currentConfig = cfgRes.data;
    if (langRes.ok) {
        languagesDetail = langRes.data || [];
        allLanguages = languagesDetail.map(l => l.code || l);
    }
    // Rebuild the local pair state from the server so a removed pair doesn't
    // linger in the UI. The user's in-progress edits to *other* pairs are
    // preserved via a shallow merge on keys that still exist server-side.
    const serverPairs = currentConfig?.language_pairs || {};
    const merged = {};
    for (const [k, v] of Object.entries(serverPairs)) {
        merged[k] = pairsState[k] ? { ...v, ...pairsState[k] } : v;
    }
    pairsState = merged;

    populateNewPairForm();
    renderPairs();
    renderLanguages();
}

// ---------------------------------------------------------------------------
// Namespaces panel (rename / delete — same API as Translations modal).
// ---------------------------------------------------------------------------

function setSettingsNsError(msg) {
    const el = document.getElementById('settingsNsError');
    if (!el) return;
    if (msg) {
        el.textContent = msg;
        el.classList.remove('hidden');
    } else {
        el.textContent = '';
        el.classList.add('hidden');
    }
}

async function refreshSettingsNamespaceList() {
    const list = document.getElementById('settingsNsList');
    const empty = document.getElementById('settingsNsEmpty');
    if (!list || !empty) return;

    list.innerHTML = '<div class="py-6 text-center text-xs text-slate-500 dark:text-slate-400">Loading…</div>';
    empty.classList.add('hidden');
    setSettingsNsError('');

    const resp = await api.getNamespaceStats();
    if (!resp.ok) {
        console.error('getNamespaceStats:', resp.error);
        setSettingsNsError(`Failed to load namespaces: ${resp.error}`);
        list.innerHTML = '';
        return;
    }
    settingsNsEntries = Array.isArray(resp.data?.namespaces) ? resp.data.namespaces : [];

    if (!settingsNsEntries.length) {
        list.innerHTML = '';
        empty.classList.remove('hidden');
        return;
    }
    renderSettingsNamespaceList();
}

function renderSettingsNamespaceList() {
    const list = document.getElementById('settingsNsList');
    if (!list) return;

    list.innerHTML = settingsNsEntries.map(entry => `
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
                <button type="button" class="px-2 py-1 rounded-md text-xs font-medium bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-600 text-slate-700 dark:text-slate-200 hover:bg-slate-100 dark:hover:bg-slate-700"
                        data-action="rename">Rename</button>
                <button type="button" class="px-2 py-1 rounded-md text-xs font-medium bg-white dark:bg-slate-800 border border-red-300 dark:border-red-700 text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-900/30"
                        data-action="delete">Delete</button>
            </div>
            <div class="hidden items-center gap-1" data-role="actions-edit">
                <button type="button" class="px-2 py-1 rounded-md text-xs font-medium bg-indigo-600 hover:bg-indigo-700 text-white"
                        data-action="rename-save">Save</button>
                <button type="button" class="px-2 py-1 rounded-md text-xs font-medium bg-slate-200 dark:bg-slate-700 text-slate-700 dark:text-slate-200 hover:bg-slate-300 dark:hover:bg-slate-600"
                        data-action="rename-cancel">Cancel</button>
            </div>
        </div>
    `).join('');

    list.querySelectorAll('.ns-manage-row').forEach(row => {
        const ns = row.dataset.ns;
        row.querySelector('[data-action="rename"]').addEventListener('click', () => enterSettingsNsRenameMode(row));
        row.querySelector('[data-action="rename-cancel"]').addEventListener('click', () => exitSettingsNsRenameMode(row));
        row.querySelector('[data-action="rename-save"]').addEventListener('click', () => saveSettingsNsRename(row, ns));
        row.querySelector('[data-action="delete"]').addEventListener('click', () => deleteSettingsNamespace(ns));
        row.querySelector('[data-role="edit"]').addEventListener('keydown', (e) => {
            if (e.key === 'Enter') { e.preventDefault(); saveSettingsNsRename(row, ns); }
            if (e.key === 'Escape') { e.preventDefault(); exitSettingsNsRenameMode(row); }
        });
    });
}

function enterSettingsNsRenameMode(row) {
    row.querySelector('[data-role="display"]').classList.add('hidden');
    row.querySelector('[data-role="actions-view"]').classList.add('hidden');
    const edit = row.querySelector('[data-role="edit"]');
    edit.classList.remove('hidden');
    const actions = row.querySelector('[data-role="actions-edit"]');
    actions.classList.remove('hidden');
    actions.classList.add('flex');
    setTimeout(() => { edit.focus(); edit.select(); }, 0);
}

function exitSettingsNsRenameMode(row) {
    row.querySelector('[data-role="display"]').classList.remove('hidden');
    row.querySelector('[data-role="actions-view"]').classList.remove('hidden');
    const edit = row.querySelector('[data-role="edit"]');
    edit.classList.add('hidden');
    edit.value = row.dataset.ns;
    const actions = row.querySelector('[data-role="actions-edit"]');
    actions.classList.add('hidden');
    actions.classList.remove('flex');
}

async function saveSettingsNsRename(row, oldNs) {
    const newNs = row.querySelector('[data-role="edit"]').value.trim();
    if (!newNs) {
        setSettingsNsError('New namespace name is required.');
        return;
    }
    if (!/^[A-Za-z0-9._-]+$/.test(newNs)) {
        setSettingsNsError('Namespace may only contain letters, digits, dot, dash, underscore.');
        return;
    }
    if (newNs === oldNs) {
        exitSettingsNsRenameMode(row);
        return;
    }

    setSettingsNsError('');
    const saveBtn = row.querySelector('[data-action="rename-save"]');
    saveBtn.disabled = true;
    saveBtn.textContent = 'Saving…';

    const resp = await api.renameNamespace(oldNs, newNs);
    if (!resp.ok) {
        console.error('renameNamespace:', resp.error);
        setSettingsNsError(resp.error || `Rename failed: ${oldNs} → ${newNs}`);
        saveBtn.disabled = false;
        saveBtn.textContent = 'Save';
        return;
    }

    Utils.showToast(`Renamed ${oldNs} → ${newNs}`, 'success');
    await refreshAfterNamespaceChange();
}

async function deleteSettingsNamespace(ns) {
    const entry = settingsNsEntries.find(e => e.namespace === ns);
    const keyCount = entry ? entry.key_count : 0;
    const msg = keyCount
        ? `Delete namespace "${ns}"? This removes ${keyCount} key${keyCount === 1 ? '' : 's'} across every language. This cannot be undone.`
        : `Delete namespace "${ns}"? This cannot be undone.`;
    if (!confirm(msg)) return;

    setSettingsNsError('');
    const resp = await api.deleteNamespace(ns);
    if (!resp.ok) {
        console.error('deleteNamespace:', resp.error);
        setSettingsNsError(resp.error || `Delete failed: ${ns}`);
        return;
    }

    Utils.showToast(`Deleted namespace ${ns}`, 'success');
    await refreshAfterNamespaceChange();
}

async function refreshAfterNamespaceChange() {
    const langRes = await api.getLanguages();
    if (langRes.ok) {
        languagesDetail = langRes.data || [];
        allLanguages = languagesDetail.map(l => l.code || l);
        renderLanguages();
    }
    await refreshSettingsNamespaceList();
}

function setupEventHandlers() {
    document.getElementById('savePairsBtn').addEventListener('click', savePairs);
    document.getElementById('addPairBtn').addEventListener('click', addPair);

    const addLangBtn = document.getElementById('addLangBtn');
    if (addLangBtn) addLangBtn.addEventListener('click', addLanguage);
    const newLangCode = document.getElementById('newLangCode');
    if (newLangCode) {
        newLangCode.addEventListener('keydown', (e) => {
            if (e.key === 'Enter') {
                e.preventDefault();
                addLanguage();
            }
        });
    }
}

function addPair() {
    const src = document.getElementById('newPairSource').value;
    const tgt = document.getElementById('newPairTarget').value;
    const model = document.getElementById('newPairModel').value;
    const temp = parseFloat(document.getElementById('newPairTemp').value) || 0.2;

    if (!src || !tgt) {
        Utils.showToast('Pick a source and target language', 'error');
        return;
    }
    if (src === tgt) {
        Utils.showToast('Source and target must differ', 'error');
        return;
    }
    const key = `${src}_${tgt}`;
    if (pairsState[key]) {
        Utils.showToast(`Pair ${src} → ${tgt} already exists`, 'error');
        return;
    }

    pairsState[key] = {
        model: model || '',
        temperature: temp,
        confidence_threshold: 0.7,
    };
    renderPairs();
}

async function savePairs() {
    try {
        const result = await api.updateConfig({ language_pairs: pairsState });
        if (!result.ok) {
            Utils.showToast(`Failed to save pairs: ${result.error}`, 'error');
            return;
        }
        Utils.showToast('✓ Language pairs saved', 'success');
        currentConfig = { ...currentConfig, language_pairs: pairsState };
    } catch (error) {
        console.error('Error saving pairs:', error);
        Utils.showToast('Error saving pairs', 'error');
    }
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
