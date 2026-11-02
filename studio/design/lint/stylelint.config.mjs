// The design drift guard for every web surface's CSS (studio/design/lint/README.md).
// Colours, type sizes, faces and radii come from studio/design/tokens through
// var(); literal values are what drift looks like.
export default {
  // Stylelint's own correctness rules (unknown properties, units and
  // selectors, duplicates, invalid colours), as Biome checked the apps' CSS.
  extends: ["stylelint-config-recommended"],
  plugins: ["stylelint-declaration-strict-value"],
  ignoreFiles: [
    // Copies of studio/design/ synced into the apps, checked by their own sync --check.
    "../../../code/apps/forge/apps/web/src/design/**",
    "../../../code/apps/layer/src/design/**",
    "../../../code/apps/glyph/apps/web/src/design/**",
    "../../../code/apps/raven/apps/web/src/design/**",
    "../../../code/apps/almanac/apps/web/src/design/**",
    "../../../code/apps/postrider/apps/web/src/design/**",
    // The phone's page: the mobile redesign is deferred (studio/design/layouts/app-window.md).
    "../../../code/apps/layer/src/phone/**",
    // Layer's theme sources: its extra themes (Tokyo Night, Cursor, Ayu) and
    // profile colours fill the token names with their own palettes there.
    "../../../code/apps/layer/src/theme.css",
  ],
  rules: {
    // Tailwind's own at-rules, in the web apps' styles.css.
    "at-rule-no-unknown": [
      true,
      {
        ignoreAtRules: [
          "theme",
          "source",
          "utility",
          "variant",
          "custom-variant",
          "apply",
          "reference",
          "plugin",
          "config",
        ],
      },
    ],
    // It compares selectors that only end alike (`.post a` after
    // `.products-menu li a`), or a base rule after an override more specific
    // than it (`.sidebar` after `.app.no-sidebar > .sidebar`). Neither
    // changes what renders; none of its 42 findings was a bug.
    "no-descending-specificity": null,
    "color-no-hex": true,
    "color-named": "never",
    // Literal colour functions; color-mix() over var() is the tint recipe.
    "function-disallowed-list": [
      "rgb",
      "rgba",
      "hsl",
      "hsla",
      "hwb",
      "lab",
      "lch",
      "oklab",
      "oklch",
    ],
    "scale-unlimited/declaration-strict-value": [
      ["/color$/", "background", "fill", "stroke", "font-size", "font-family", "border-radius"],
      {
        ignoreValues: [
          "transparent",
          "currentColor",
          "currentcolor",
          "inherit",
          "initial",
          "unset",
          "none",
          "0",
          "/^[0-9.]+(em|%)$/",
        ],
        ignoreFunctions: true,
        disableFix: true,
        message:
          'Use a design token (var(--…)) for ${property}, not "${value}": studio/design/tokens.',
      },
    ],
  },
};
