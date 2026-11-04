import { bootApp } from "@code-villain/web";
import { keepFilesOut } from "./drag";
import { router } from "./router";
import { loadSession } from "./session";
import "./styles.css";

keepFilesOut();
bootApp(router, loadSession);
