// Boots every studio web app the same way: Nightshade from the system
// preference, the session, external links on desktop, clicked links that
// let go of the focus, then the router.

import { type AnyRouter, RouterProvider } from "@tanstack/react-router";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { openExternalLinksInBrowser } from "./desktop";

/** Follow the system's explicit light preference; Nightshade night is the default. */
export function followSystemTheme(): void {
  const lightMode = matchMedia("(prefers-color-scheme: light)");
  const applyTheme = () => {
    document.documentElement.dataset.theme = lightMode.matches ? "day" : "night";
  };
  applyTheme();
  lightMode.addEventListener("change", applyTheme);
}

/**
 * A link clicked with the pointer lets go of the focus it took. Kept, the
 * next key (G then I, Esc, J) would ring it, a sidebar item or a list row,
 * long after the page moved on. A link reached with Tab or opened with
 * Enter keeps it, as keys should.
 */
function letClickedLinksGo(): void {
  document.addEventListener("click", (event) => {
    // A click from the keys has no pointer behind it.
    if (event.detail === 0) return;
    const link = (event.target as Element | null)?.closest?.("a[href]");
    if (link instanceof HTMLElement && link === document.activeElement) link.blur();
  });
}

/** Theme, session, desktop link handling, then mount `router` on `#root`. */
export function bootApp(router: AnyRouter, loadSession: () => void | Promise<void>): void {
  followSystemTheme();
  void loadSession();
  openExternalLinksInBrowser();
  letClickedLinksGo();
  const root = document.getElementById("root");
  if (!root) throw new Error("missing #root");
  createRoot(root).render(
    <StrictMode>
      <RouterProvider router={router} />
    </StrictMode>,
  );
}
