// Import page: configure → preview (backend /import/preview) → POST /import.

let selectedFile = null;
let previewData = null;
// The step-2 primary names what it will do: "Import 42 keys".
let importLabel = 'Import';

document.addEventListener('DOMContentLoaded', () => {

    const fileInput = document.getElementById('importFile');
    const langInput = document.getElementById('importLang');
    const nsInput = document.getElementById('importDefaultNs');
    const btnToReview = document.getElementById('btnToReview');
    const btnBack = document.getElementById('btnBack');
    const btnRunImport = document.getElementById('btnRunImport');
    const dropZone = document.getElementById('importDropZone');
    const fileName = document.getElementById('importFileName');

    fileInput.addEventListener('change', () => {
        const f = fileInput.files?.[0];
        selectedFile = f || null;
        if (!f) {
            fileName.textContent = 'Choose a JSON file';
            btnToReview.disabled = true;
            return;
        }
        fileName.textContent = f.name;

        const parsed = parseFilenameHints(f.name);
        if (parsed.lang) langInput.value = parsed.lang;
        if (parsed.ns) nsInput.value = parsed.ns;

        btnToReview.disabled = false;
    });

    // Dropping a file on the zone is the same as choosing it.
    dropZone.addEventListener('dragover', (e) => {
        e.preventDefault();
        dropZone.classList.add('is-dragging');
    });
    dropZone.addEventListener('dragleave', () => dropZone.classList.remove('is-dragging'));
    dropZone.addEventListener('drop', (e) => {
        e.preventDefault();
        dropZone.classList.remove('is-dragging');
        const f = e.dataTransfer?.files?.[0];
        if (!f) return;
        const dt = new DataTransfer();
        dt.items.add(f);
        fileInput.files = dt.files;
        fileInput.dispatchEvent(new Event('change'));
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
        btnToReview.textContent = 'Reading the file…';

        const res = await api.importPreview(f, defaultNS);
        btnToReview.disabled = false;
        btnToReview.textContent = 'Preview';

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
        btnRunImport.textContent = importLabel;

        if (!result.ok) {
            Utils.showToast(`Import failed: ${result.error}`, 'error');
            return;
        }

        const breakdown = Array.isArray(result.data.namespaces) ? result.data.namespaces : [];
        if (breakdown.length > 1) {
            const summary = breakdown.map(b => `${b.namespace} (${b.count})`).join(', ');
            Utils.showToast(
                `Imported ${result.data.count} keys into ${lang}: ${summary}`,
                'success'
            );
        } else {
            const landedNS = breakdown[0]?.namespace || defaultNS;
            Utils.showToast(`Imported ${result.data.count} keys into ${lang}/${landedNS}`, 'success');
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

    s1.classList.toggle('hidden', step !== 1);
    s2.classList.toggle('hidden', step === 1);
    const [current, other] = step === 1 ? [b1, b2] : [b2, b1];
    current.setAttribute('aria-current', 'step');
    other.removeAttribute('aria-current');

    if (step === 1) {
        document.getElementById('importLeadTitle').textContent = 'Import a JSON file';
        document.getElementById('importLeadSub').innerHTML =
            'Check where each key lands, including locize-style <span class="code">ns:key</span> prefixes, then merge.';
    }
}

function escapeHtml(s) {
    return Utils.escapeHtml(String(s ?? ''));
}

function renderReview(lang, defaultNS, data) {
    const count = data.count ?? 0;
    const breakdown = Array.isArray(data.namespaces) ? data.namespaces : [];
    const rows = Array.isArray(data.sample_rows) ? data.sample_rows : [];

    const keys = `${count} key${count === 1 ? '' : 's'}`;
    importLabel = `Import ${keys}`;
    document.getElementById('btnRunImport').textContent = importLabel;
    document.getElementById('importLeadTitle').textContent = `Review ${keys}`;
    document.getElementById('importLeadSub').textContent =
        `${selectedFile ? selectedFile.name + ' · ' : ''}to merge into ${Utils.langInfo(lang).name}.`;

    document.getElementById('reviewSummary').innerHTML =
        `Keys with an <span class="code">ns:key</span> prefix land in that namespace; ` +
        `the rest in <span class="code">${escapeHtml(defaultNS)}</span>.`;

    const bdEl = document.getElementById('reviewNsBreakdown');
    bdEl.innerHTML = breakdown.map(b =>
        `<span class="ui-chip">${escapeHtml(b.namespace)} <span class="text-faint">${b.count}</span></span>`
    ).join('');

    document.getElementById('sampleHint').textContent =
        rows.length > 0 ? `· first ${rows.length}, sorted` : '';

    const tbody = document.getElementById('reviewTableBody');
    tbody.innerHTML = rows.map(r => `
        <tr>
            <td class="text-muted">${escapeHtml(r.namespace)}</td>
            <td class="font-mono text-code">${escapeHtml(r.key)}</td>
            <td class="font-mono text-code text-muted break-all">${escapeHtml(r.raw_key)}</td>
            <td class="import-value truncate" title="${escapeHtml(r.value_preview)}">${escapeHtml(r.value_preview)}</td>
        </tr>
    `).join('');

    if (rows.length === 0) {
        tbody.innerHTML = `<tr><td colspan="4" class="empty">No rows. Is the file empty?</td></tr>`;
    }
}
