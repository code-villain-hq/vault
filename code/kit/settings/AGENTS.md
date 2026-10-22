# Settings: notes for coding agents

`code-villain-settings` reads an app server's settings: one TOML file, with
the environment over it. Forge, Glyph, Raven and Layer use it. A company
runs an app with a file like `<app>.example.toml` in the app's folder.

- The file is `--config <file>`, else the one `<APP>_CONFIG` names, else
  `./<app>.toml` when there is one.
- Each setting has a name in the environment: the app, the section and the
  key in capitals. `[oidc] issuer` is `FORGE_OIDC_ISSUER`. The environment
  wins over the file, so a container can keep its secrets out of the file.
- A setting the app doesn't know, in the file, stops the server and is
  named: a typo would otherwise be a setting quietly not set. Each app
  lists what it knows (`KNOWN` in its `config.rs`).
- `require` stops when any of the settings it's given isn't set, naming
  them all at once.
- Lists become comma-separated text; `flag` takes true/false, 1/0, yes/no
  and on/off.

The desktop app's own server and tests use `Settings::from_env`: no file.

`cargo test` here.
