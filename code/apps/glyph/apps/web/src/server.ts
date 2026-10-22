// Where the API is, and how requests prove who's asking: the kit's places
// (`@code-villain/web`), named for Glyph.
import { configureApp } from "@code-villain/web";

export const DEFAULT_SERVER = "http://localhost:7490";

configureApp({
  key: "glyph",
  name: "Glyph",
  invoke: async <T>(command: string, args?: Record<string, unknown>) => {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(command, args);
  },
});

export {
  apiUrl,
  authHeaders,
  authPlace,
  bundled,
  credentialsFor,
  forgetPlace,
  getPlaces,
  loadPlaces,
  macUserName,
  normalizeServer,
  type Place,
  placeFor,
  placeName,
  placeWorkspaces,
  publicUrl,
  rememberToken,
  serverUrl,
  setCurrentPlace,
  setServerUrl,
  socketProtocols,
  startOnThisMac,
} from "@code-villain/web";
