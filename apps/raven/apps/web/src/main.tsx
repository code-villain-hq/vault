import { RouterProvider } from "@tanstack/react-router";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { router } from "./router";
import { loadSession } from "./session";
import "./styles.css";

// Follow the system's explicit light preference; Nightshade night is the default.
const lightMode = matchMedia("(prefers-color-scheme: light)");
const applyTheme = () => {
  document.documentElement.dataset.theme = lightMode.matches ? "day" : "night";
};
applyTheme();
lightMode.addEventListener("change", applyTheme);

void loadSession();

const root = document.getElementById("root");
if (!root) throw new Error("missing #root");
createRoot(root).render(
  <StrictMode>
    <RouterProvider router={router} />
  </StrictMode>,
);
