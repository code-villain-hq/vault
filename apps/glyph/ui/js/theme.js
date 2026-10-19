// Loaded in <head>, right after the Tailwind CDN script, so the first frame
// already has the right tone.
//
// Night or day follows the system, as in Forge: ui/design/color.css fills the
// token names for each.
(function () {
    const light = matchMedia('(prefers-color-scheme: light)');
    const apply = () => { document.documentElement.dataset.theme = light.matches ? 'day' : 'night'; };
    apply();
    light.addEventListener('change', apply);
})();

// Tailwind stays for layout only. Its palette, type scale, radii and shadows
// are replaced by the design tokens, so `bg-slate-800` or `text-indigo-400`
// generate nothing: colour classes are the token names (`bg-panel`,
// `text-muted`, `text-danger-ink`). Preflight is off because its unlayered
// reset would beat the shared ui-* controls in their `components` layer;
// css/glyph.css carries the reset in `@layer base` instead.
tailwind.config = {
    corePlugins: { preflight: false },
    theme: {
        colors: {
            transparent: 'transparent',
            current: 'currentColor',
            bg: 'var(--bg)',
            panel: 'var(--panel)',
            surface: 'var(--surface)',
            hover: 'var(--hover)',
            line: 'var(--line)',
            selected: 'var(--selected)',
            dialog: 'var(--dialog)',
            field: 'var(--field)',
            fg: 'var(--fg)',
            muted: 'var(--muted)',
            faint: 'var(--faint)',
            primary: 'var(--primary)',
            'on-primary': 'var(--on-primary)',
            accent: 'var(--accent)',
            signal: 'var(--signal)',
            ok: 'var(--ok)',
            warn: 'var(--warn)',
            danger: 'var(--danger)',
            info: 'var(--info)',
            'ok-ink': 'var(--status-ok-ink)',
            'warn-ink': 'var(--status-warn-ink)',
            'danger-ink': 'var(--status-danger-ink)',
            'info-ink': 'var(--status-info-ink)',
        },
        fontFamily: {
            sans: 'var(--font-reading)',
            mono: 'var(--font-mono)',
            display: 'var(--font-display)',
        },
        fontSize: {
            title: ['var(--type-title-size)', 'var(--type-title-leading)'],
            ui: ['var(--type-ui-size)', 'var(--type-ui-leading)'],
            'ui-small': ['var(--type-ui-small-size)', 'var(--type-ui-small-leading)'],
            control: ['var(--type-control-size)', 'var(--type-control-leading)'],
            code: ['var(--type-ui-code-size)', 'var(--type-ui-code-leading)'],
        },
        borderRadius: {
            none: '0',
            sm: 'var(--radius-sm)',
            DEFAULT: 'var(--radius-md)',
            md: 'var(--radius-md)',
            lg: 'var(--radius-lg)',
            full: 'var(--radius-full)',
        },
        boxShadow: { none: 'none' },
        extend: {},
    },
};
