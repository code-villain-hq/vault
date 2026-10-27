import React from "react";
import ReactDOM from "react-dom/client";
import "@xterm/xterm/css/xterm.css";
import "./theme.css";
import "./styles.css";
import "./styles-acp.css";
import "./styles-spec.css";
import "./styles-epic.css";
import "./styles-runs.css";
import "./styles-profiles.css";
import "./styles-observability.css";
import "./styles-settings.css";
import App from "./App";
import { Splash } from "./components/Splash";
import { fontsReady } from "./lib/terminal";
import { paintRemembered } from "./lib/theme";

paintRemembered();

// The mark breathes from the first frame (WIN-4); the app itself waits for
// the fonts, which xterm measures its cells with.
const root = ReactDOM.createRoot(document.getElementById("root")!);
root.render(<Splash />);
void fontsReady().then(() => root.render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
));
