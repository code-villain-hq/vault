// Dashboard page. Groups language coverage by namespace (locize-style).
let currentConfig = null;
let allLanguages = [];
let allNamespaces = [];

document.addEventListener('DOMContentLoaded', async () => {
    await initDashboard();
});

async function initDashboard() {
    try {
        const configResult = await api.getConfig();
        if (!configResult.ok) {
            Utils.showToast('Failed to load configuration', 'error');
            return;
        }
        currentConfig = configResult.data;

        const langResult = await api.getLanguages();
        if (!langResult.ok) {
            Utils.showToast('Failed to load languages', 'error');
            return;
        }
        // /api/languages now returns [{code, translation_count, namespaces}]
        allLanguages = (langResult.data || []).map(l => (typeof l === 'string' ? { code: l, namespaces: [] } : l));

        const nsResult = await api.getNamespaces();
        allNamespaces = nsResult.ok ? (nsResult.data.namespaces || []) : [];

        await renderDashboard();

        setupKeyboardShortcuts();
    } catch (error) {
        console.error('Error initializing dashboard:', error);
        Utils.showToast('Error initializing dashboard', 'error');
    }
}

async function renderDashboard() {
    const container = document.getElementById('languagePairsContainer');
    container.innerHTML = '';

    const sourceLang = currentConfig.user_preferences.source_language;
    const sourceInfo = Utils.langInfo(sourceLang);
    const targetLangs = allLanguages.map(l => l.code).filter(code => code !== sourceLang);

    // Fresh DB: nothing to cover yet, so the one action is getting strings in.
    if (allNamespaces.length === 0) {
        setLead('No translations yet', 'Import a JSON file, or run make seed-demo and restart.', 'Import JSON', '/ui/import.html');
        return;
    }

    // Source keys per namespace, for the totals.
    let totalSourceKeys = 0;
    for (const ns of allNamespaces) {
        const r = await api.getTranslations(sourceLang, ns);
        totalSourceKeys += r.ok ? Object.keys(r.data || {}).length : 0;
    }

    if (targetLangs.length === 0) {
        setLead(`Only ${sourceInfo.name} so far`,
            `${plural(totalSourceKeys, 'key')} in ${plural(allNamespaces.length, 'namespace')}. Add a target language to start translating.`,
            'Add a language', '/ui/settings.html');
        return;
    }

    // Every source key is one slot per target language, so translated +
    // missing always adds up to the slots.
    const totalSlots = totalSourceKeys * targetLangs.length;
    let totalTranslated = 0;
    let totalMissing = 0;
    let totalLowConfidence = 0;
    const rows = [];

    for (const targetLang of targetLangs) {
        const nsStats = [];
        let translated = 0, missing = 0, lowConf = 0, total = 0;

        for (const ns of allNamespaces) {
            const statsResult = await api.getTranslationStats(targetLang, sourceLang, ns);
            if (!statsResult.ok) continue;
            const s = statsResult.data;
            nsStats.push({ ns, ...s });
            total += s.total_keys || 0;
            translated += s.translated_keys || 0;
            missing += s.missing_count || 0;
            lowConf += s.low_confidence_count || 0;
        }

        totalTranslated += translated;
        totalMissing += missing;
        totalLowConfidence += lowConf;
        rows.push({ lang: targetLang, translated, missing, lowConf, total, nsStats });
    }

    // The language that needs the most work leads.
    rows.sort((a, b) => (b.missing + b.lowConf) - (a.missing + a.lowConf) || a.lang.localeCompare(b.lang));
    container.innerHTML = rows.map(languageRow).join('');

    const label = document.getElementById('languagesLabel');
    label.textContent = `Languages · from ${sourceInfo.name}`;
    label.hidden = false;

    const across = `${totalTranslated} of ${totalSlots} translated across ${plural(targetLangs.length, 'language')} · ${plural(totalSourceKeys, 'key')} in ${plural(allNamespaces.length, 'namespace')}`;
    if (totalMissing > 0) {
        const low = totalLowConfidence > 0 ? ` · ${totalLowConfidence} low confidence` : '';
        setLead(`${plural(totalMissing, 'translation')} missing`, across + low, 'Review missing', '/ui/missing.html');
    } else if (totalLowConfidence > 0) {
        setLead(`${plural(totalLowConfidence, 'translation')} with low confidence`, across, 'Review', '/ui/missing.html');
    } else {
        setLead('Every translation is in', across);
    }
}

// One row per target language: coverage, a namespace breakdown, and a status
// pill only where something needs a look.
function languageRow(row) {
    const info = Utils.langInfo(row.lang);
    const pct = row.total > 0 ? (row.translated / row.total) * 100 : 0;
    const ns = row.nsStats.map(s => `${Utils.escapeHtml(s.ns)} ${s.translated_keys}/${s.total_keys}`).join(' · ');
    const pills = [
        row.missing > 0 ? `<span class="ui-status ui-status-warn">${row.missing} missing</span>` : '',
        row.lowConf > 0 ? `<span class="ui-status">${row.lowConf} low confidence</span>` : '',
    ].join('');
    const needsReview = row.missing > 0 || row.lowConf > 0;
    return `
        <div class="lang-cover ui-row">
            <div><span class="lang-cover-name">${Utils.escapeHtml(info.name)}</span><span class="lang-cover-code">${Utils.escapeHtml(row.lang)}</span></div>
            <div class="progress" role="progressbar" aria-label="${Utils.escapeHtml(info.name)} coverage" aria-valuenow="${Math.round(pct)}" aria-valuemin="0" aria-valuemax="100"><span style="width: ${pct}%"></span></div>
            <div class="lang-cover-done">${row.translated} of ${row.total}</div>
            <div class="lang-cover-ns" title="${ns}">${ns}</div>
            <div class="lang-cover-pills">${pills}</div>
            ${needsReview
                ? `<a class="lang-cover-link ui-link text-control" href="/ui/missing.html?target=${encodeURIComponent(row.lang)}">Review</a>`
                : '<span></span>'}
        </div>
    `;
}

function setLead(title, sub, actionLabel, actionHref) {
    document.getElementById('leadTitle').textContent = title;
    document.getElementById('leadSub').textContent = sub;
    const action = document.getElementById('leadAction');
    action.hidden = !actionLabel;
    if (actionLabel) {
        action.textContent = actionLabel;
        action.href = actionHref;
    }
}

function plural(n, word) {
    return `${n} ${word}${n === 1 ? '' : 's'}`;
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
