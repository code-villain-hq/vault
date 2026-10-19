// Utility Functions
//
// Language display metadata (shared by Translations, Dashboard, etc.).
// Extend when you add locales; unknown codes fall back to 🌐 and the raw code.
const LANG_INFO = {
    en: { name: 'English',    flag: '🇬🇧' },
    us: { name: 'English',    flag: '🇺🇸' },
    de: { name: 'German',     flag: '🇩🇪' },
    sv: { name: 'Swedish',    flag: '🇸🇪' },
    fi: { name: 'Finnish',    flag: '🇫🇮' },
    fr: { name: 'French',     flag: '🇫🇷' },
    es: { name: 'Spanish',    flag: '🇪🇸' },
    it: { name: 'Italian',    flag: '🇮🇹' },
    pt: { name: 'Portuguese', flag: '🇵🇹' },
    nl: { name: 'Dutch',      flag: '🇳🇱' },
    pl: { name: 'Polish',     flag: '🇵🇱' },
    ru: { name: 'Russian',    flag: '🇷🇺' },
    ja: { name: 'Japanese',   flag: '🇯🇵' },
    zh: { name: 'Chinese',    flag: '🇨🇳' },
    ko: { name: 'Korean',     flag: '🇰🇷' },
    ar: { name: 'Arabic',     flag: '🇸🇦' },
    tr: { name: 'Turkish',    flag: '🇹🇷' },
    cs: { name: 'Czech',      flag: '🇨🇿' },
    hu: { name: 'Hungarian',  flag: '🇭🇺' },
    no: { name: 'Norwegian',  flag: '🇳🇴' },
    da: { name: 'Danish',     flag: '🇩🇰' },
    is: { name: 'Icelandic',  flag: '🇮🇸' },
    el: { name: 'Greek',      flag: '🇬🇷' },
    he: { name: 'Hebrew',     flag: '🇮🇱' },
    uk: { name: 'Ukrainian',  flag: '🇺🇦' },
    ro: { name: 'Romanian',   flag: '🇷🇴' },
    bg: { name: 'Bulgarian',  flag: '🇧🇬' },
    th: { name: 'Thai',       flag: '🇹🇭' },
    vi: { name: 'Vietnamese', flag: '🇻🇳' },
    id: { name: 'Indonesian', flag: '🇮🇩' },
};

function normalizeLangCode(code) {
    return String(code ?? '').trim().toLowerCase();
}

const Utils = {
    // Toast: the dialog tone with a status icon; it fades after `duration`.
    showToast(message, type = 'info', duration = 4000) {
        const toastContainer = document.getElementById('toastContainer');
        const toast = document.createElement('div');
        const glyph = { success: 'check', error: 'alert', warning: 'alert', info: 'info' }[type] || 'info';

        toast.className = 'toast';
        toast.dataset.tone = type;
        toast.setAttribute('role', type === 'error' ? 'alert' : 'status');
        toast.innerHTML = `${icon(glyph)}<span>${Utils.escapeHtml(message)}</span>`;
        toast.style.animationDelay = `0s, ${Math.max(0, duration - 200)}ms`;

        toastContainer.appendChild(toast);

        setTimeout(() => {
            toast.remove();
        }, duration);
    },

    // Format percentage
    formatPercentage(value) {
        return `${Math.round(value * 100) / 100}%`;
    },

    // Format date
    formatDate(dateString) {
        if (!dateString) return 'N/A';
        try {
            const date = new Date(dateString);
            if (isNaN(date.getTime())) return 'N/A';
            return date.toLocaleString();
        } catch (e) {
            return 'N/A';
        }
    },

    // Format confidence as percentage with color
    formatConfidence(confidence) {
        const percent = Math.round(confidence * 100);
        return `${percent}%`;
    },

    // Status pill classes for a confidence (design/components.md#status-pill).
    getConfidenceColor(confidence) {
        if (confidence >= 0.8) return 'ui-status ui-status-ok';
        if (confidence >= 0.6) return 'ui-status ui-status-warn';
        return 'ui-status ui-status-danger';
    },

    // Create language pair key
    createLanguagePairKey(source, target) {
        return `${source}_${target}`;
    },

    // Parse language pair key
    parseLanguagePairKey(key) {
        const [source, target] = key.split('_');
        return { source, target };
    },

    /** Display name + regional flag emoji for a BCP-47-ish language code. */
    langInfo(code) {
        const key = normalizeLangCode(code);
        if (LANG_INFO[key]) {
            return LANG_INFO[key];
        }
        const base = key.split(/[-_]/)[0];
        if (base && base !== key && LANG_INFO[base]) {
            return { name: `${LANG_INFO[base].name} (${key})`, flag: LANG_INFO[base].flag };
        }
        return { name: key || '?', flag: '🌐' };
    },

    // Show loading spinner
    showLoading(element, text = 'Loading…') {
        element.innerHTML = `
            <div class="empty flex items-center justify-center gap-3">
                <span class="spinner" aria-hidden="true"></span>
                <span>${text}</span>
            </div>
        `;
    },

    // Clear content
    clear(element) {
        element.innerHTML = '';
    },

    // Parse query string
    parseQueryString() {
        const params = new URLSearchParams(window.location.search);
        return Object.fromEntries(params);
    },

    // Debounce function
    debounce(func, wait) {
        let timeout;
        return function executedFunction(...args) {
            const later = () => {
                clearTimeout(timeout);
                func(...args);
            };
            clearTimeout(timeout);
            timeout = setTimeout(later, wait);
        };
    },

    // Throttle function
    throttle(func, limit) {
        let lastFunc;
        let lastRan;
        return function(...args) {
            if (!lastRan) {
                func.apply(this, args);
                lastRan = Date.now();
            } else {
                clearTimeout(lastFunc);
                lastFunc = setTimeout(() => {
                    if ((Date.now() - lastRan) >= limit) {
                        func.apply(this, args);
                        lastRan = Date.now();
                    }
                }, limit - (Date.now() - lastRan));
            }
        };
    },

    // Smooth scroll
    smoothScroll(element) {
        element.scrollIntoView({ behavior: 'smooth' });
    },

    // Copy to clipboard
    copyToClipboard(text) {
        navigator.clipboard.writeText(text).then(() => {
            this.showToast('Copied to clipboard!', 'success');
        });
    },

    // HTML-escape a string for safe inclusion in innerHTML. Handles the five
    // entities that can break out of text context (`& < > " '`). Nullish
    // inputs become an empty string so the caller doesn't have to null-check.
    // Historically this was duplicated verbatim in translations.js, missing.js
    // and settings.js — they now all alias `Utils.escapeHtml`.
    escapeHtml(s) {
        return String(s ?? '')
            .replace(/&/g, '&amp;')
            .replace(/</g, '&lt;')
            .replace(/>/g, '&gt;')
            .replace(/"/g, '&quot;')
            .replace(/'/g, '&#39;');
    }
};

// Static markup writes its icons as <span data-icon="name" data-icon-class="icon-sm">;
// draw them once. Page scripts load after this one, so their markup is ready.
document.querySelectorAll('[data-icon]').forEach(el => {
    el.outerHTML = icon(el.dataset.icon, el.dataset.iconClass || '');
});

// A page's #confirmModal (title, body, Cancel, OK) as a promise: true on OK,
// false on Cancel, Escape or a click on the backdrop. Focus returns to the
// control that opened it.
function confirmDialog({ title, body, confirmLabel }) {
    const modal = document.getElementById('confirmModal');
    const ok = document.getElementById('confirmOk');
    const cancel = document.getElementById('confirmCancel');
    document.getElementById('confirmTitle').textContent = title;
    document.getElementById('confirmBody').textContent = body;
    ok.textContent = confirmLabel;
    const opener = document.activeElement;
    modal.hidden = false;
    cancel.focus();
    return new Promise(resolve => {
        const done = (value) => {
            modal.hidden = true;
            ok.removeEventListener('click', onOk);
            cancel.removeEventListener('click', onCancel);
            modal.removeEventListener('click', onBackdrop);
            document.removeEventListener('keydown', onKey, true);
            if (opener && opener.focus) opener.focus();
            resolve(value);
        };
        const onOk = () => done(true);
        const onCancel = () => done(false);
        const onBackdrop = (e) => { if (e.target === modal) done(false); };
        const onKey = (e) => { if (e.key === 'Escape') { e.preventDefault(); done(false); } };
        ok.addEventListener('click', onOk);
        cancel.addEventListener('click', onCancel);
        modal.addEventListener('click', onBackdrop);
        document.addEventListener('keydown', onKey, true);
    });
}
