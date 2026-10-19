// The app frame's sidebar (design/layouts/app-window.md): the wordmark, then
// one item per view. Each page puts this script inside its
// <aside class="app-side">, so the sidebar is drawn before the page paints,
// and names itself with <body data-page="…">.
(function () {
    const views = [
        ['dashboard', '/', 'dashboard', 'Dashboard'],
        ['translations', '/ui/translations.html', 'languages', 'Translations'],
        ['missing', '/ui/missing.html', 'review', 'Review'],
        ['glossary', '/ui/glossary.html', 'book', 'Glossary'],
        ['import', '/ui/import.html', 'import', 'Import'],
        ['settings', '/ui/settings.html', 'settings', 'Settings'],
    ];
    const current = document.body.dataset.page;
    const side = document.currentScript.parentElement;
    side.insertAdjacentHTML('beforeend', `
        <div class="app-side-top"><a href="/" class="app-wordmark">Glyph</a></div>
        <nav class="app-nav" aria-label="Views">
            ${views.map(([page, href, ic, label]) => `
                <a href="${href}" class="app-nav-item" ${page === current ? 'aria-current="page"' : ''}>
                    ${icon(ic)}<span>${label}</span>
                </a>`).join('')}
        </nav>
    `);
})();
