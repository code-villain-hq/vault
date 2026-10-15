import { RouterProvider } from "@tanstack/react-router";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { openExternalLinksInBrowser } from "./platform";
import { router } from "./router";
import { loadSession } from "./session";
import "@fontsource-variable/inter";
import "./styles.css";

void loadSession();
openExternalLinksInBrowser();

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");
createRoot(root).render(
  <StrictMode>
    <RouterProvider router={router} />
  </StrictMode>,
);
