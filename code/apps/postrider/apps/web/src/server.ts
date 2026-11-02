// Where the API is, and how requests prove who's asking: the kit's places
// (`@code-villain/web`), named for Postrider. Postrider has two: this Mac,
// in the desktop app, and "here" in development.
import { configureApp } from "@code-villain/web";

configureApp({
  key: "postrider",
  name: "Postrider",
  invoke: async <T>(command: string, args?: Record<string, unknown>) => {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(command, args);
  },
});

export {
  authHeaders,
  credentialsFor,
  type Place,
  setCurrentPlace,
  socketProtocols,
} from "@code-villain/web";
