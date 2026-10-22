// Set by each app's vite.config.ts from its Cargo.toml.
declare const __APP_VERSION__: string;

const version = typeof __APP_VERSION__ === "string" ? __APP_VERSION__ : "dev";

/**
 * Settings end with the app's name and version ("Forge 0.1.0"), as
 * behaviour.md says: last in the settings sidebar, in every app.
 */
export function AppVersion({ name }: { name: string }) {
  return (
    <p className="app-version mt-auto px-2 pt-6 text-xs text-faint">
      {name} {version}
    </p>
  );
}
