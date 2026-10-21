/**
 * What a terminal does when it comes back further behind than its pane's
 * scrollback reaches (PANE-8), by which screen it is on.
 *
 * A line-by-line program (a shell, Claude Code) is replayed from what the
 * scrollback still holds onto a wiped terminal, and reads right. A
 * full-screen one (OpenCode) draws only what changed since its last frame:
 * the wipe took its screen, and with it the alternate screen and mouse modes
 * it set once at start, and the window came back black but for whatever
 * changed after. A resize put it right, because a resize is what makes such
 * a program draw everything again. So its screen is kept, and it is asked to.
 */
export function whenBehind(screen: "normal" | "alternate"): { wipe: boolean; repaint: boolean } {
  return screen === "alternate" ? { wipe: false, repaint: true } : { wipe: true, repaint: false };
}
