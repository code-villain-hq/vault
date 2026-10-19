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
    // Toast notification system
    showToast(message, type = 'info', duration = 3000) {
        const toastContainer = document.getElementById('toastContainer');
        const toast = document.createElement('div');
        
        const bgColor = {
            'success': 'bg-green-500 dark:bg-green-600',
            'error': 'bg-red-500 dark:bg-red-600',
            'info': 'bg-blue-500 dark:bg-blue-600',
            'warning': 'bg-amber-500 dark:bg-amber-600'
        }[type] || 'bg-blue-500 dark:bg-blue-600';

        const icon = {
            'success': '✓',
            'error': '✕',
            'info': 'ℹ',
            'warning': '!'
        }[type] || 'ℹ';

        toast.className = `toast ${bgColor} text-white px-4 py-3 rounded-lg shadow-lg flex items-center space-x-3 min-w-max`;
        toast.innerHTML = `
            <span class="text-lg font-bold">${icon}</span>
            <span>${message}</span>
        `;

        toastContainer.appendChild(toast);

        setTimeout(() => {
            toast.remove();
        }, duration);
    },

    // Dark mode is the only supported theme. The inline <head> script on
    // every page already adds `dark` to <html>; this function exists only
    // to defensively re-apply it in case a page forgot the inline script or
    // some third-party script removed the class. `setTheme` is kept as a
    // no-op shim so any legacy callers (e.g. older cached bundles) don't
    // throw.
    initDarkMode() {
        document.documentElement.classList.add('dark');
    },

    setTheme() {
        document.documentElement.classList.add('dark');
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

    // Get confidence color
    getConfidenceColor(confidence) {
        if (confidence >= 0.8) return 'bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-400';
        if (confidence >= 0.6) return 'bg-yellow-100 dark:bg-yellow-900/30 text-yellow-700 dark:text-yellow-400';
        return 'bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-400';
    },

    // Get confidence meter fill color
    getConfidenceMeterColor(confidence) {
        if (confidence >= 0.8) return 'bg-green-500';
        if (confidence >= 0.6) return 'bg-yellow-500';
        return 'bg-red-500';
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
    showLoading(element, text = 'Loading...') {
        element.innerHTML = `
            <div class="flex items-center justify-center space-x-3 py-8">
                <div class="w-8 h-8 border-4 border-indigo-300 dark:border-indigo-600 border-t-indigo-600 dark:border-t-indigo-300 rounded-full animate-spin"></div>
                <span class="text-slate-600 dark:text-slate-400">${text}</span>
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
    },

    // Set active page in navigation
    setActivePage(pageName) {
        // Remove active state from all nav links
        document.querySelectorAll('.nav-link').forEach(link => {
            link.classList.remove('text-indigo-600', 'dark:text-indigo-400', 'font-semibold', 'border-b-2', 'border-indigo-600', 'dark:border-indigo-400');
            link.classList.add('text-slate-600', 'dark:text-slate-400');
        });

        // Add active state to current page
        const activeLink = document.querySelector(`[data-page="${pageName}"]`);
        if (activeLink) {
            activeLink.classList.remove('text-slate-600', 'dark:text-slate-400');
            activeLink.classList.add('text-indigo-600', 'dark:text-indigo-400', 'font-semibold', 'border-b-2', 'border-indigo-600', 'dark:border-indigo-400');
        }
    }
};

// Initialize dark mode on load
document.addEventListener('DOMContentLoaded', () => {
    Utils.initDarkMode();
});
