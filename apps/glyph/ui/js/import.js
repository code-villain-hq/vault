// Import page: configure → preview (backend /import/preview) → POST /import.

let selectedFile = null;
let previewData = null;

document.addEventListener('DOMContentLoaded', () => {
    const fileInput = document.getElementById('importFile');
    const langInput = document.getElementById('importLang');
    const nsInput = document.getElementById('importDefaultNs');
    const btnToReview = document.getElementById('btnToReview');
    const btnBack = document.getElementById('btnBack');
    const btnRunImport = document.getElementById('btnRunImport');

    fileInput.addEventListener('change', () => {
        const f = fileInput.files?.[0];
        selectedFile = f || null;
        if (!f) {
            btnToReview.disabled = true;
            return;
        }

        const parsed = parseFilenameHints(f.name);
        if (parsed.lang) langInput.value = parsed.lang;
        if (parsed.ns) nsInput.value = parsed.ns;

        btnToReview.disabled = false;
    });

    btnToReview.addEventListener('click', async () => {
        const f = fileInput.files?.[0];
        if (!f) {
            Utils.showToast('Choose a JSON file first', 'error');
            return;
        }
        const lang = (langInput.value || '').trim().toLowerCase();
        if (!/^[a-z]{2,3}$/.test(lang)) {
            Utils.showToast('Enter a valid language code (2–3 letters, e.g. en, sv)', 'error');
            return;
        }
        langInput.value = lang;

        const defaultNS = (nsInput.value || '').trim() || 'translation';
        nsInput.value = defaultNS;

        btnToReview.disabled = true;
        btnToReview.textContent = 'Parsing…';

        const res = await api.importPreview(f, defaultNS);
        btnToReview.disabled = false;
        btnToReview.textContent = 'Continue to review →';

        if (!res.ok) {
            Utils.showToast(`Preview failed: ${res.error}`, 'error');
            return;
        }

        previewData = res.data;
        selectedFile = f;
        renderReview(lang, defaultNS, previewData);
        showStep(2);
    });

    btnBack.addEventListener('click', () => {
        showStep(1);
        previewData = null;
    });

    btnRunImport.addEventListener('click', async () => {
        const f = fileInput.files?.[0];
        const lang = (langInput.value || '').trim().toLowerCase();
        const defaultNS = (nsInput.value || '').trim() || 'translation';
        if (!f || !previewData) {
            Utils.showToast('Nothing to import — go back and preview again', 'error');
            return;
        }

        btnRunImport.disabled = true;
        btnRunImport.textContent = 'Importing…';

        const result = await api.importTranslations(f, lang, 'merge', defaultNS);
        btnRunImport.disabled = false;
        btnRunImport.textContent = '✓ Import now';

        if (!result.ok) {
            Utils.showToast(`Import failed: ${result.error}`, 'error');
            return;
        }

        const breakdown = Array.isArray(result.data.namespaces) ? result.data.namespaces : [];
        if (breakdown.length > 1) {
            const summary = breakdown.map(b => `${b.namespace} (${b.count})`).join(', ');
            Utils.showToast(
                `✓ Imported ${result.data.count} keys into ${lang}: ${summary}`,
                'success'
            );
        } else {
            const landedNS = breakdown[0]?.namespace || defaultNS;
            Utils.showToast(`✓ Imported ${result.data.count} keys into ${lang}/${landedNS}`, 'success');
        }

        window.location.href = '/';
    });

    document.addEventListener('keydown', (e) => {
        const isMeta = e.metaKey || e.ctrlKey;
        if (isMeta && e.key === '1') { e.preventDefault(); window.location.href = '/'; return; }
        if (isMeta && e.key === '2') { e.preventDefault(); window.location.href = '/ui/translations.html'; return; }
        if (isMeta && e.key === '3') { e.preventDefault(); window.location.href = '/ui/missing.html'; return; }
        if (isMeta && e.key === '4') { e.preventDefault(); window.location.href = '/ui/glossary.html'; return; }
        if (isMeta && e.key === '5') { e.preventDefault(); window.location.href = '/ui/settings.html'; return; }
        if (isMeta && e.key === '6') { e.preventDefault(); window.location.href = '/ui/import.html'; return; }
    });
});

/** @returns {{ lang: string, ns: string }} */
function parseFilenameHints(filename) {
    const base = filename.replace(/\.json$/i, '');
    let lang = base;
    let ns = 'translation';
    const m = base.match(/^([a-z]{2,3})[._-](.+)$/i);
    if (m) {
        lang = m[1].toLowerCase();
        ns = m[2];
    }
    return { lang, ns };
}

function showStep(step) {
    const s1 = document.getElementById('importStep1');
    const s2 = document.getElementById('importStep2');
    const b1 = document.getElementById('step1Badge');
    const b2 = document.getElementById('step2Badge');

    if (step === 1) {
        s1.classList.remove('hidden');
        s2.classList.add('hidden');
        b1.className = 'px-3 py-1 rounded-full bg-indigo-600 text-white font-medium';
        b2.className = 'px-3 py-1 rounded-full bg-slate-700 text-slate-300';
    } else {
        s1.classList.add('hidden');
        s2.classList.remove('hidden');
        b1.className = 'px-3 py-1 rounded-full bg-slate-700 text-slate-300';
        b2.className = 'px-3 py-1 rounded-full bg-indigo-600 text-white font-medium';
    }
}

function escapeHtml(s) {
    return Utils.escapeHtml(String(s ?? ''));
}

function renderReview(lang, defaultNS, data) {
    const count = data.count ?? 0;
    const breakdown = Array.isArray(data.namespaces) ? data.namespaces : [];
    const rows = Array.isArray(data.sample_rows) ? data.sample_rows : [];

    document.getElementById('reviewSummary').innerHTML =
        `About to merge <strong class="text-white">${count}</strong> entries into ` +
        `<span class="font-mono text-indigo-300">${escapeHtml(lang)}</span>. ` +
        `Default namespace for unprefixed keys: <span class="font-mono text-slate-300">${escapeHtml(defaultNS)}</span>.`;

    const bdEl = document.getElementById('reviewNsBreakdown');
    bdEl.innerHTML = breakdown.map(b =>
        `<span class="inline-flex items-center gap-1 px-2 py-1 rounded-md bg-slate-700 text-xs font-mono text-slate-200">${escapeHtml(b.namespace)} <span class="text-slate-400">(${b.count})</span></span>`
    ).join('');

    document.getElementById('sampleHint').textContent =
        rows.length > 0 ? `(first ${rows.length} keys, sorted)` : '';

    const tbody = document.getElementById('reviewTableBody');
    tbody.innerHTML = rows.map(r => `
        <tr class="hover:bg-slate-800/50">
            <td class="px-4 py-2 font-mono text-xs text-violet-300">${escapeHtml(r.namespace)}</td>
            <td class="px-4 py-2 font-mono text-xs text-slate-200">${escapeHtml(r.key)}</td>
            <td class="px-4 py-2 font-mono text-xs text-slate-500 break-all">${escapeHtml(r.raw_key)}</td>
            <td class="px-4 py-2 text-xs text-slate-400 max-w-md truncate" title="${escapeHtml(r.value_preview)}">${escapeHtml(r.value_preview)}</td>
        </tr>
    `).join('');

    if (rows.length === 0) {
        tbody.innerHTML = `<tr><td colspan="4" class="px-4 py-6 text-center text-slate-500">No rows (empty file?)</td></tr>`;
    }
}
