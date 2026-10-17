import React from "react";
import ReactDOM from "react-dom/client";
import "@xterm/xterm/css/xterm.css";
import "./theme.css";
import "./styles.css";
import "./styles-acp.css";
import "./styles-spec.css";
import "./styles-runs.css";
import "./styles-profiles.css";
import "./styles-observability.css";
import App from "./App";
import { fontsReady } from "./lib/terminal";
import { paintRemembered } from "./lib/theme";

paintRemembered();

void fontsReady().then(() => ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
));
