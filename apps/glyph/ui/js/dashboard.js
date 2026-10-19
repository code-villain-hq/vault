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
    const targetLangs = allLanguages.map(l => l.code).filter(code => code !== sourceLang);

    // The summary cards count *translation slots*, not source keys: every
    // source key contributes one slot per target language. With this scale,
    // Translated + Missing == Total Keys, so the cards always add up no
    // matter how many target languages exist.
    let totalSourceKeys = 0;
    let totalSlots = 0;
    let totalTranslated = 0;
    let totalMissing = 0;
    let totalLowConfidence = 0;

    // If there are no namespaces yet (fresh DB), show a friendly empty state.
    if (allNamespaces.length === 0) {
        container.innerHTML = `
            <div class="md:col-span-2 lg:col-span-3 bg-white dark:bg-slate-800 rounded-xl p-8 text-center border border-dashed border-slate-300 dark:border-slate-700">
                <p class="text-slate-500 dark:text-slate-400">No translations yet. Run <code class="bg-slate-100 dark:bg-slate-900 px-2 py-1 rounded">make seed-demo</code> or <a href="/ui/import.html" class="text-indigo-400 hover:underline">import a JSON file</a>.</p>
            </div>
        `;
        document.getElementById('totalKeysCount').textContent = '0';
        document.getElementById('translatedCount').textContent = '0';
        document.getElementById('missingCount').textContent = '0';
        document.getElementById('lowConfidenceCount').textContent = '0';
        const sub = document.querySelector('[data-summary="total-keys-subtitle"]');
        if (sub) sub.textContent = '';
        return;
    }

    // Aggregate source key counts per namespace (used for totals).
    const sourceKeysPerNs = {};
    for (const ns of allNamespaces) {
        const r = await api.getTranslations(sourceLang, ns);
        sourceKeysPerNs[ns] = r.ok ? Object.keys(r.data || {}).length : 0;
        totalSourceKeys += sourceKeysPerNs[ns];
    }
    totalSlots = totalSourceKeys * targetLangs.length;

    for (const targetLang of targetLangs) {
        // Per-namespace stats for this target language.
        const nsStats = [];
        let langTranslated = 0;
        let langMissing = 0;
        let langLowConf = 0;
        let langTotal = 0;

        for (const ns of allNamespaces) {
            const statsResult = await api.getTranslationStats(targetLang, sourceLang, ns);
            if (!statsResult.ok) continue;
            const s = statsResult.data;

            nsStats.push({ ns, ...s });
            langTotal += s.total_keys || 0;
            langTranslated += s.translated_keys || 0;
            langMissing += s.missing_count || 0;
            langLowConf += s.low_confidence_count || 0;
        }

        totalTranslated += langTranslated;
        totalMissing += langMissing;
        totalLowConfidence += langLowConf;

        const coverage = langTotal > 0 ? (langTranslated / langTotal) * 100 : 0;
        const coverageColor = coverage >= 80 ? 'bg-green-500' : coverage >= 50 ? 'bg-yellow-500' : 'bg-red-500';

        const targetInfo = Utils.langInfo(targetLang);
        const sourceInfo = Utils.langInfo(sourceLang);

        const nsRows = nsStats.map(s => {
            const pct = (s.coverage_percentage || 0).toFixed(0);
            const pctColor = (s.coverage_percentage || 0) >= 80 ? 'text-green-600 dark:text-green-400'
                : (s.coverage_percentage || 0) >= 50 ? 'text-yellow-600 dark:text-yellow-400'
                : 'text-red-600 dark:text-red-400';
            return `
                <div class="flex items-center justify-between py-1.5 text-xs">
                    <span class="font-mono text-slate-600 dark:text-slate-400 truncate">${s.ns}</span>
                    <div class="flex items-center gap-2">
                        <span class="${pctColor} font-semibold">${pct}%</span>
                        <span class="text-slate-500 dark:text-slate-500">(${s.translated_keys}/${s.total_keys})</span>
                    </div>
                </div>
            `;
        }).join('');

        const card = document.createElement('div');
        card.className = 'bg-white dark:bg-slate-800 rounded-xl p-6 shadow-sm border border-slate-200 dark:border-slate-700 hover:shadow-md transition-all duration-200';
        card.innerHTML = `
            <div class="mb-4 min-w-0">
                <h3 class="text-lg font-bold text-slate-900 dark:text-white flex items-center gap-2 flex-wrap">
                    <span class="text-2xl leading-none shrink-0" aria-hidden="true">${targetInfo.flag}</span>
                    <span>${targetInfo.name} <span class="text-slate-500 dark:text-slate-400 font-semibold text-sm">(${targetLang})</span></span>
                </h3>
                <p class="text-xs text-slate-500 dark:text-slate-400 mt-1 flex items-center gap-1 flex-wrap">
                    <span aria-hidden="true">${sourceInfo.flag}</span><span class="font-mono">${sourceLang}</span>
                    <span>→</span>
                    <span aria-hidden="true">${targetInfo.flag}</span><span class="font-mono">${targetLang}</span>
                </p>
            </div>

            <div class="mb-4">
                <div class="flex justify-between mb-2">
                    <span class="text-sm font-medium text-slate-600 dark:text-slate-400">Coverage</span>
                    <span class="text-sm font-bold text-slate-900 dark:text-white">${coverage.toFixed(1)}%</span>
                </div>
                <div class="w-full bg-slate-200 dark:bg-slate-700 rounded-full h-2">
                    <div class="${coverageColor} h-2 rounded-full transition-all duration-300" style="width: ${coverage}%"></div>
                </div>
            </div>

            <div class="grid grid-cols-2 gap-3 mb-4 text-sm">
                <div class="bg-slate-50 dark:bg-slate-900 rounded-lg p-3">
                    <p class="text-slate-600 dark:text-slate-400 text-xs">Translated</p>
                    <p class="text-lg font-bold text-green-600 dark:text-green-400">${langTranslated}</p>
                </div>
                <div class="bg-slate-50 dark:bg-slate-900 rounded-lg p-3">
                    <p class="text-slate-600 dark:text-slate-400 text-xs">Missing</p>
                    <p class="text-lg font-bold text-amber-600 dark:text-amber-400">${langMissing}</p>
                </div>
            </div>

            <div class="border-t border-slate-200 dark:border-slate-700 pt-3 mb-4">
                <p class="text-xs uppercase tracking-wide text-slate-500 dark:text-slate-400 font-semibold mb-1">Namespaces</p>
                <div class="divide-y divide-slate-100 dark:divide-slate-900">${nsRows}</div>
            </div>

            <div class="text-xs text-slate-500 dark:text-slate-400 mb-4">
                Low Confidence: <span class="font-semibold text-red-600 dark:text-red-400">${langLowConf}</span>
            </div>

            <a href="/ui/missing.html?target=${targetLang}" class="w-full block text-center px-4 py-2 bg-gradient-to-r from-indigo-600 to-indigo-700 hover:from-indigo-700 hover:to-indigo-800 text-white rounded-lg font-medium transition-all duration-200 text-sm">
                Review →
            </a>
        `;
        container.appendChild(card);
    }

    if (targetLangs.length === 0) {
        container.innerHTML = `
            <div class="md:col-span-2 lg:col-span-3 bg-white dark:bg-slate-800 rounded-xl p-8 text-center border border-dashed border-slate-300 dark:border-slate-700">
                <p class="text-slate-500 dark:text-slate-400">Only the source language (${sourceLang}) exists. <a href="/ui/import.html" class="text-indigo-400 hover:underline">Import</a> a target language JSON to see coverage.</p>
            </div>
        `;
    }

    document.getElementById('totalKeysCount').textContent = totalSlots;
    document.getElementById('translatedCount').textContent = totalTranslated;
    document.getElementById('missingCount').textContent = totalMissing;
    document.getElementById('lowConfidenceCount').textContent = totalLowConfidence;

    // Surface the underlying source-key count as a subtitle so users still
    // see "you have 37 unique keys"; the headline scale is now slots.
    const totalCard = document.querySelector('[data-summary="total-keys-subtitle"]');
    if (totalCard) {
        totalCard.textContent = `${totalSourceKeys} keys × ${targetLangs.length} target${targetLangs.length === 1 ? '' : 's'}`;
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
