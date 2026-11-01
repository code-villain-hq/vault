// Boots every studio web app the same way: Nightshade from the system
// preference, the session, external links on desktop, then the router.

import { RouterProvider, type AnyRouter } from "@tanstack/react-router";
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

/** Theme, session, desktop link handling, then mount `router` on `#root`. */
export function bootApp(router: AnyRouter, loadSession: () => void | Promise<void>): void {
  followSystemTheme();
  void loadSession();
  openExternalLinksInBrowser();
  const root = document.getElementById("root");
  if (!root) throw new Error("missing #root");
  createRoot(root).render(
    <StrictMode>
      <RouterProvider router={router} />
    </StrictMode>,
  );
}
