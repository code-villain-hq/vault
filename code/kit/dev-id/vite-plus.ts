// The studio's Vite+ settings: how every app's code is formatted (Oxfmt) and
// linted (Oxlint). Each app's root vite.config.ts spreads them and adds what
// it ignores; each kit's does the same. Plain objects, so this kit needs no
// packages of its own.

// `ignore` names what is generated or copied, never edited by hand.
export function fmt(ignore: string[] = []) {
  return {
    printWidth: 100,
    // One block, packages before the app's own files, as the code had them.
    sortImports: { newlinesBetween: false },
    // Markdown is prose, wrapped and aligned by hand.
    ignorePatterns: [...ignore, "**/*.md"],
  };
}

type Plugin = "typescript" | "unicorn" | "oxc" | "react" | "jsx-a11y";

// `confirm` says what to use instead of `window.confirm`.
export function lint(confirm = "ConfirmDialog from @code-villain/web") {
  const message = `Embedded webviews (the desktop app) can silently refuse it. Use ${confirm}.`;
  const plugins: Plugin[] = ["typescript", "unicorn", "oxc", "react", "jsx-a11y"];
  return {
    plugins,
    categories: { correctness: "error" as const },
    // Type-aware rules, and TypeScript's own errors: `vp check` is the type
    // check. A directive that silences nothing has to go.
    options: { typeAware: true, typeCheck: true, reportUnusedDisableDirectives: "error" as const },
    rules: {
      "no-restricted-globals": error({ name: "confirm", message }),
      "no-restricted-properties": error({ object: "window", property: "confirm", message }),
      // Beyond Oxlint's correctness rules: what the apps were held to before.
      "react/rules-of-hooks": "error" as const,
      "react/exhaustive-deps": "error" as const,
      "react/no-array-index-key": "error" as const,
      "react/no-danger": "error" as const,
      "typescript/no-explicit-any": "error" as const,
      "unicorn/prefer-math-min-max": "error" as const,
      "no-constructor-return": "error" as const,
      // A parameter named `_place` is unused on purpose; so is a key taken out
      // of an object by destructuring it, as `{ [id]: _, ...rest }`.
      "no-unused-vars": error({ argsIgnorePattern: "^_", ignoreRestSiblings: true }),
      // `[...text].length` counts characters, not UTF-16 units, on purpose.
      "typescript/no-misused-spread": error({ allow: ["string"] }),
      // These check code for the React Compiler, which no app uses.
      "react/purity": "off" as const,
      "react/preserve-manual-memoization": "off" as const,
      "react/refs": "off" as const,
      "react/set-state-in-effect": "off" as const,
      "react/immutability": "off" as const,
      "react/globals": "off" as const,
      "react/static-components": "off" as const,
      // A dialog moves focus into itself when it opens, as WAI-ARIA's dialog
      // pattern says: its first field takes `autoFocus`.
      "jsx-a11y/no-autofocus": "off" as const,
      // Menus, listboxes and live regions take their ARIA roles on purpose.
      "jsx-a11y/prefer-tag-over-role": "off" as const,
      // Forms take ⌘↩ on their fields. A click on a row is checked by
      // click-events-have-key-events instead.
      "jsx-a11y/no-noninteractive-element-interactions": "off" as const,
    },
    overrides: [
      {
        files: ["**/*.test.ts", "**/*.test.tsx"],
        // Bun's types say `expect(…).rejects` returns nothing; it returns a
        // promise, and the test has to await it.
        rules: { "typescript/await-thenable": "off" as const },
      },
    ],
  };
}

// A rule that fails the check, with its options, as the tuple Oxlint takes.
function error<T>(options: T): ["error", T] {
  return ["error", options];
}
