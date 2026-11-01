import { bootApp } from "@code-villain/web";
import { router } from "./router";
import { loadSession } from "./session";
import "./styles.css";

bootApp(router, loadSession);
