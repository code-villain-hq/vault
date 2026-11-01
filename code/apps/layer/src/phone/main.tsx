import React from "react";
import ReactDOM from "react-dom/client";
import "@xterm/xterm/css/xterm.css";
import "../theme.css";
import "./phone.css";
import { fontsReady } from "../lib/terminal";
import { paintRemembered } from "../lib/theme";
import { App } from "./App";

paintRemembered();

void fontsReady().then(() =>
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  ),
);
