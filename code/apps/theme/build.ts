// Generate the VS Code themes and Neovim's palette from the vault's tokens:
// studio/design/tokens/syntax.resolved.json for code, color.resolved.json for the
// editor's frame. `--check` writes nothing and fails if an output has drifted.

const tokens = new URL("../../../studio/design/tokens/", import.meta.url);
const read = async (name: string) => JSON.parse(await Bun.file(new URL(name, tokens)).text());
const syntax = await read("syntax.resolved.json");
const color = await read("color.resolved.json");

type Mode = "night" | "day";
const modes: Mode[] = ["night", "day"];
const NONE = "#00000000";

/** The terminal's 16 colours: one table in studio/design/tokens for every terminal. */
function ansi(mode: Mode): { normal: string[]; bright: string[] } {
  return syntax[mode].ansi;
}

/** VS Code: TextMate scopes from the shared table, then semantic tokens and the workbench. */
function vscode(mode: Mode) {
  const s: Record<string, string> = syntax[mode].syntax;
  const u: Record<string, string> = syntax[mode].ui;
  const t: Record<string, string> = color[mode];
  const tokenColors = Object.entries(syntax.scopes as Record<string, string[]>).map(([role, scope]) => ({
    name: role,
    scope,
    settings: {
      foreground: s[role],
      ...(syntax.styles[role] ? { fontStyle: syntax.styles[role] } : {}),
    },
  }));
  const role = (name: string) => ({ foreground: s[name] });
  const semanticTokenColors = {
    namespace: role("text"),
    type: role("type"),
    class: role("type"),
    enum: role("type"),
    interface: role("type"),
    struct: role("type"),
    typeParameter: role("type"),
    builtinType: role("builtin"),
    parameter: role("text"),
    variable: role("text"),
    property: role("property"),
    enumMember: role("constant"),
    event: role("property"),
    function: role("function"),
    method: role("function"),
    macro: role("function"),
    label: role("text"),
    comment: role("comment"),
    string: role("string"),
    keyword: role("keyword"),
    selfKeyword: role("special"),
    number: role("constant"),
    regexp: role("constant"),
    operator: role("operator"),
    decorator: role("keyword"),
    lifetime: role("constant"),
    formatSpecifier: role("constant"),
    "*.deprecated": { strikethrough: true },
  };
  const colors: Record<string, string> = {
    // The frame: bars and side panels on bg, the code on panel.
    foreground: t.fg,
    descriptionForeground: t.muted,
    disabledForeground: t.faint,
    errorForeground: u.danger,
    focusBorder: t.accent,
    "widget.border": NONE,
    "widget.shadow": NONE,
    "selection.background": u.selection,
    "textLink.foreground": t.fg,
    "textLink.activeForeground": t.fg,
    "titleBar.activeBackground": t.bg,
    "titleBar.activeForeground": t.muted,
    "titleBar.inactiveBackground": t.bg,
    "titleBar.inactiveForeground": t.faint,
    "titleBar.border": NONE,
    "activityBar.background": t.bg,
    "activityBar.foreground": t.fg,
    "activityBar.inactiveForeground": t.muted,
    "activityBar.border": NONE,
    "activityBar.activeBorder": NONE,
    "activityBar.activeBackground": t.selected,
    "activityBarBadge.background": t.signal,
    "activityBarBadge.foreground": t["on-primary"],
    "sideBar.background": t.bg,
    "sideBar.foreground": t.muted,
    "sideBar.border": NONE,
    "sideBarTitle.foreground": t.muted,
    "sideBarSectionHeader.background": t.bg,
    "sideBarSectionHeader.foreground": t.muted,
    "sideBarSectionHeader.border": NONE,
    "list.activeSelectionBackground": t.selected,
    "list.activeSelectionForeground": t.fg,
    "list.inactiveSelectionBackground": t.selected,
    "list.inactiveSelectionForeground": t.fg,
    "list.hoverBackground": t.hover,
    "list.hoverForeground": t.fg,
    "list.focusBackground": t.selected,
    "list.focusOutline": t.accent,
    "list.focusAndSelectionOutline": t.accent,
    "list.inactiveFocusOutline": NONE,
    "list.highlightForeground": t.fg,
    "list.focusHighlightForeground": t.fg,
    "list.errorForeground": u.danger,
    "list.warningForeground": u.warn,
    "tree.indentGuidesStroke": t.line,
    "editorGroupHeader.tabsBackground": t.bg,
    "editorGroupHeader.tabsBorder": NONE,
    "editorGroup.border": t.bg,
    "tab.activeBackground": u.background,
    "tab.activeForeground": t.fg,
    "tab.inactiveBackground": t.bg,
    "tab.inactiveForeground": t.muted,
    "tab.hoverBackground": t.hover,
    "tab.border": t.bg,
    "tab.activeBorder": NONE,
    "tab.activeBorderTop": NONE,
    "tab.unfocusedActiveForeground": t.muted,
    "tab.activeModifiedBorder": t.signal,
    "breadcrumb.background": u.background,
    "breadcrumb.foreground": t.muted,
    "breadcrumb.focusForeground": t.fg,
    "breadcrumb.activeSelectionForeground": t.fg,
    "statusBar.background": t.bg,
    "statusBar.foreground": t.muted,
    "statusBar.border": NONE,
    "statusBar.noFolderBackground": t.bg,
    "statusBar.debuggingBackground": t.bg,
    "statusBar.debuggingForeground": u.warn,
    "statusBarItem.hoverBackground": t.hover,
    "statusBarItem.remoteBackground": t.bg,
    "statusBarItem.remoteForeground": t.muted,
    "panel.background": t.bg,
    "panel.border": NONE,
    "panelTitle.activeForeground": t.fg,
    "panelTitle.inactiveForeground": t.muted,
    "panelTitle.activeBorder": t.fg,
    "badge.background": t.surface,
    "badge.foreground": t.muted,
    "button.background": t.primary,
    "button.foreground": t["on-primary"],
    "button.hoverBackground": t["primary-hover"],
    "button.secondaryBackground": t.surface,
    "button.secondaryForeground": t.fg,
    "button.secondaryHoverBackground": t.hover,
    "input.background": t.field,
    "input.foreground": t.fg,
    "input.border": NONE,
    "input.placeholderForeground": t.faint,
    "inputOption.activeBorder": t.accent,
    "inputOption.activeBackground": u.highlight,
    "dropdown.background": t.dialog,
    "dropdown.border": NONE,
    "dropdown.foreground": t.fg,
    "quickInput.background": t.dialog,
    "quickInput.foreground": t.fg,
    "quickInputList.focusBackground": u.highlight,
    "quickInputList.focusForeground": t.fg,
    "pickerGroup.foreground": t.muted,
    "pickerGroup.border": NONE,
    "menu.background": t.dialog,
    "menu.foreground": t.fg,
    "menu.selectionBackground": u.highlight,
    "menu.selectionForeground": t.fg,
    "menu.separatorBackground": t.line,
    "menu.border": NONE,
    "notifications.background": t.dialog,
    "notifications.foreground": t.fg,
    "notifications.border": NONE,
    "notificationCenterHeader.background": t.dialog,
    "scrollbar.shadow": NONE,
    "scrollbarSlider.background": `${t.fg}1A`,
    "scrollbarSlider.hoverBackground": `${t.fg}2E`,
    "scrollbarSlider.activeBackground": `${t.fg}40`,
    "progressBar.background": t.signal,
    "terminal.background": t.bg,
    "terminal.foreground": s.text,
    "terminalCursor.foreground": u.cursor,
    "terminal.selectionBackground": u.selection,
    // Ansi colours from the shared terminal table.
    ...Object.fromEntries(
      ["Black", "Red", "Green", "Yellow", "Blue", "Magenta", "Cyan", "White"].flatMap((name, i) => [
        [`terminal.ansi${name}`, ansi(mode).normal[i]],
        [`terminal.ansiBright${name}`, ansi(mode).bright[i]],
      ]),
    ),
    // The code.
    "editor.background": u.background,
    "editor.foreground": s.text,
    "editorCursor.foreground": u.cursor,
    "editor.lineHighlightBackground": u.cursorline,
    "editor.lineHighlightBorder": NONE,
    "editorLineNumber.foreground": u["line-number"],
    "editorLineNumber.activeForeground": u["line-number-current"],
    "editor.selectionBackground": u.selection,
    "editor.inactiveSelectionBackground": u.cursorline,
    "editor.selectionHighlightBackground": u.cursorline,
    "editor.wordHighlightBackground": u.cursorline,
    "editor.wordHighlightStrongBackground": u.cursorline,
    "editor.findMatchBackground": u["search-current"],
    "editor.findMatchForeground": u["on-search-current"],
    "editor.findMatchHighlightBackground": u.search,
    "editor.findRangeHighlightBackground": u.cursorline,
    "editor.rangeHighlightBackground": u.cursorline,
    "editorBracketMatch.background": NONE,
    "editorBracketMatch.border": u.match,
    "editorBracketHighlight.foreground1": s.punctuation,
    "editorBracketHighlight.foreground2": s.punctuation,
    "editorBracketHighlight.foreground3": s.punctuation,
    "editorBracketHighlight.unexpectedBracket.foreground": u.danger,
    "editorIndentGuide.background1": u.guide,
    "editorIndentGuide.activeBackground1": u["line-number"],
    "editorWhitespace.foreground": u.guide,
    "editorRuler.foreground": u.guide,
    "editorCodeLens.foreground": u["line-number"],
    "editorInlayHint.foreground": u["line-number"],
    "editorInlayHint.background": NONE,
    "editorLink.activeForeground": s.text,
    "editorGutter.background": u.background,
    "editorGutter.addedBackground": u.ok,
    "editorGutter.modifiedBackground": u.info,
    "editorGutter.deletedBackground": u.danger,
    "editorError.foreground": u.danger,
    "editorWarning.foreground": u.warn,
    "editorInfo.foreground": u.info,
    "editorHint.foreground": u.muted,
    "editorOverviewRuler.border": NONE,
    "editorOverviewRuler.findMatchForeground": u["search-current"],
    "editorOverviewRuler.addedForeground": u.ok,
    "editorOverviewRuler.modifiedForeground": u.info,
    "editorOverviewRuler.deletedForeground": u.danger,
    "editorOverviewRuler.errorForeground": u.danger,
    "editorOverviewRuler.warningForeground": u.warn,
    "editorWidget.background": u.float,
    "editorWidget.border": NONE,
    "editorHoverWidget.background": u.float,
    "editorHoverWidget.border": NONE,
    "editorSuggestWidget.background": u.float,
    "editorSuggestWidget.border": NONE,
    "editorSuggestWidget.foreground": s.text,
    "editorSuggestWidget.selectedBackground": u.highlight,
    "editorSuggestWidget.highlightForeground": s.text,
    "editorSuggestWidget.focusHighlightForeground": s.text,
    "peekView.border": t.accent,
    "peekViewEditor.background": u.float,
    "peekViewResult.background": u.float,
    "peekViewTitle.background": u.float,
    "peekViewEditor.matchHighlightBackground": u.search,
    "peekViewResult.matchHighlightBackground": u.search,
    "diffEditor.insertedLineBackground": u["diff-added"],
    "diffEditor.insertedTextBackground": u["diff-added-text"],
    "diffEditor.removedLineBackground": u["diff-removed"],
    "diffEditor.removedTextBackground": u["diff-removed-text"],
    "diffEditor.border": NONE,
    "diffEditorGutter.insertedLineBackground": u["diff-added"],
    "diffEditorGutter.removedLineBackground": u["diff-removed"],
    "gitDecoration.addedResourceForeground": u.ok,
    "gitDecoration.untrackedResourceForeground": u.ok,
    "gitDecoration.modifiedResourceForeground": u.info,
    "gitDecoration.deletedResourceForeground": u.danger,
    "gitDecoration.conflictingResourceForeground": u.warn,
    "gitDecoration.ignoredResourceForeground": t.faint,
    "minimap.background": u.background,
    "minimap.findMatchHighlight": u["search-current"],
    "minimap.selectionHighlight": u.selection,
    "minimapGutter.addedBackground": u.ok,
    "minimapGutter.modifiedBackground": u.info,
    "minimapGutter.deletedBackground": u.danger,
  };
  return {
    $schema: "vscode://schemas/color-theme",
    name: `Code Villain ${mode === "night" ? "Night" : "Day"}`,
    type: mode === "night" ? "dark" : "light",
    semanticHighlighting: true,
    semanticTokenColors,
    colors,
    tokenColors,
  };
}

/** Neovim: the resolved roles as a Lua table; the highlight groups map onto these names. */
function lua() {
  const table = (values: Record<string, string>) =>
    Object.entries(values)
      .map(([k, v]) => `    [${JSON.stringify(k)}] = ${JSON.stringify(v)},`)
      .join("\n");
  const frame = ["bg", "panel", "surface", "hover", "line", "dialog", "field", "selected", "fg", "muted", "faint", "accent", "primary", "on-primary", "signal"];
  const lines = ["-- Generated by code/apps/theme/build.ts from studio/design/tokens. Don't edit.", "return {"];
  for (const mode of modes) {
    const t: Record<string, string> = color[mode];
    lines.push(`  ${mode} = {`);
    lines.push("    -- The frame, from color.resolved.json.");
    lines.push(table(Object.fromEntries(frame.map((k) => [`frame.${k}`, t[k]]))));
    lines.push("    -- The code, from syntax.resolved.json.");
    lines.push(table(Object.fromEntries(Object.entries(syntax[mode].ui as Record<string, string>).map(([k, v]) => [`ui.${k}`, v]))));
    lines.push(table(syntax[mode].syntax));
    lines.push("    -- The terminal's 16 colours, shared with WezTerm and VS Code.");
    const { normal, bright } = ansi(mode);
    lines.push(table(Object.fromEntries([...normal, ...bright].map((c, i) => [`ansi.${i}`, c]))));
    lines.push("  },");
  }
  lines.push("}", "");
  return lines.join("\n");
}

/** WezTerm: a colour scheme per mode. The ground is the code's, so Neovim fills the window. */
function wezterm(mode: Mode) {
  const s: Record<string, string> = syntax[mode].syntax;
  const u: Record<string, string> = syntax[mode].ui;
  const t: Record<string, string> = color[mode];
  const { normal, bright } = ansi(mode);
  const q = (v: string) => JSON.stringify(v);
  const list = (values: string[]) => `[${values.map(q).join(", ")}]`;
  const tab = (name: string, fg: string, bg: string) => [`[colors.tab_bar.${name}]`, `fg_color = ${q(fg)}`, `bg_color = ${q(bg)}`, ""];
  return [
    "# Generated by code/apps/theme/build.ts from studio/design/tokens. Don't edit.",
    "[metadata]",
    `name = ${q(`Code Villain ${mode === "night" ? "Night" : "Day"}`)}`,
    'author = "Code Villain"',
    "",
    "[colors]",
    `foreground = ${q(s.text)}`,
    `background = ${q(u.background)}`,
    `cursor_bg = ${q(u.cursor)}`,
    `cursor_border = ${q(u.cursor)}`,
    `cursor_fg = ${q(u.background)}`,
    `selection_bg = ${q(u.selection)}`,
    `selection_fg = ${q(s.text)}`,
    `split = ${q(u.guide)}`,
    `scrollbar_thumb = ${q(u.guide)}`,
    `compose_cursor = ${q(s.constant)}`,
    `visual_bell = ${q(u.cursorline)}`,
    `ansi = ${list(normal)}`,
    `brights = ${list(bright)}`,
    "",
    "[colors.tab_bar]",
    `background = ${q(t.bg)}`,
    `inactive_tab_edge = ${q(t.bg)}`,
    "",
    ...tab("active_tab", s.text, u.background),
    ...tab("inactive_tab", t.muted, t.bg),
    ...tab("inactive_tab_hover", s.text, t.hover),
    ...tab("new_tab", t.muted, t.bg),
    ...tab("new_tab_hover", s.text, t.hover),
  ].join("\n");
}

const outputs: [string, string][] = [
  ...modes.map((mode): [string, string] => [`vscode/themes/code-villain-${mode}.json`, `${JSON.stringify(vscode(mode), null, 2)}\n`]),
  ["nvim/lua/code-villain/palette.lua", lua()],
  ...modes.map((mode): [string, string] => [`wezterm/code-villain-${mode}.toml`, wezterm(mode)]),
];

const here = new URL("./", import.meta.url);
if (process.argv.includes("--check")) {
  const stale: string[] = [];
  for (const [path, text] of outputs) {
    const file = Bun.file(new URL(path, here));
    if (!(await file.exists()) || (await file.text()) !== text) stale.push(path);
  }
  if (stale.length) {
    console.error(`Out of date with studio/design/tokens: ${stale.join(", ")}. Run bun run build in code/apps/theme.`);
    process.exit(1);
  }
} else {
  for (const [path, text] of outputs) await Bun.write(new URL(path, here), text);
  console.log("Built the Code Villain theme for VS Code, Neovim and WezTerm from studio/design/tokens.");
}
