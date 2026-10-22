//! An app server's settings. A company runs an app from one TOML file:
//!
//! ```toml
//! company = "Acme Inc"
//! public_url = "https://forge.acme.org"
//!
//! [oidc]
//! issuer = "https://id.acme.org"
//! client_id = "forge"
//! client_secret = "…"
//!
//! [scim]
//! token = "…"
//!
//! [database]
//! url = "postgres://forge@localhost/forge"
//! ```
//!
//! Every setting also has a name in the environment: the app's name, the
//! section and the key, in capitals. `[oidc] issuer` is `FORGE_OIDC_ISSUER`
//! and `public_url` is `FORGE_PUBLIC_URL`. The environment wins over the
//! file. A setting the app doesn't know, in the file, stops it: a typo would
//! otherwise be a setting quietly not set.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

type Env = Box<dyn Fn(&str) -> Option<String> + Send + Sync>;

pub struct Settings {
    prefix: String,
    file: Option<PathBuf>,
    values: BTreeMap<String, String>,
    env: Env,
}

impl Settings {
    /// The settings in `file`, else in the file `<APP>_CONFIG` names, else in
    /// `./<app>.toml` when there is one, with the environment over them.
    /// `known` lists the settings the app reads, as `section.key`.
    pub fn load(app: &str, known: &[&str], file: Option<&Path>) -> Result<Self, Error> {
        let prefix = app.to_uppercase();
        let file = match file {
            Some(file) => Some(file.to_path_buf()),
            None => std::env::var(format!("{prefix}_CONFIG"))
                .ok()
                .filter(|v| !v.trim().is_empty())
                .map(PathBuf::from)
                .or_else(|| {
                    let here = PathBuf::from(format!("{app}.toml"));
                    here.exists().then_some(here)
                }),
        };
        let text = match &file {
            Some(path) => Some(std::fs::read_to_string(path).map_err(|e| Error::Read {
                path: path.clone(),
                reason: e.to_string(),
            })?),
            None => None,
        };
        Self::parse(
            app,
            known,
            file,
            text.as_deref(),
            Box::new(|name| std::env::var(name).ok()),
        )
    }

    /// The environment's settings only, as the desktop app's own server and
    /// tests have them.
    pub fn from_env(app: &str) -> Self {
        Self {
            prefix: app.to_uppercase(),
            file: None,
            values: BTreeMap::new(),
            env: Box::new(|name| std::env::var(name).ok()),
        }
    }

    /// Settings from TOML text and a stand-in for the environment.
    pub fn parse(
        app: &str,
        known: &[&str],
        file: Option<PathBuf>,
        text: Option<&str>,
        env: Env,
    ) -> Result<Self, Error> {
        let prefix = app.to_uppercase();
        let mut values = BTreeMap::new();
        if let Some(text) = text {
            let name = || {
                file.clone()
                    .unwrap_or_else(|| PathBuf::from(format!("{app}.toml")))
            };
            let table: toml::Table = text.parse().map_err(|e: toml::de::Error| Error::Parse {
                path: name(),
                reason: e.message().to_string(),
            })?;
            let mut flat = Vec::new();
            flatten(None, &table, &mut flat).map_err(|key| Error::Parse {
                path: name(),
                reason: format!("{key} isn't text, a number, true or false, or a list of them"),
            })?;
            let unknown: Vec<String> = flat
                .iter()
                .map(|(key, _)| key.clone())
                .filter(|key| !known.contains(&key.as_str()))
                .collect();
            if !unknown.is_empty() {
                return Err(Error::Unknown {
                    path: name(),
                    keys: unknown,
                });
            }
            for (key, value) in flat {
                values.insert(env_name(&prefix, &key), value);
            }
        }
        Ok(Self {
            prefix,
            file,
            values,
            env,
        })
    }

    /// The setting `key` (`section.key`): from the environment, else the file.
    pub fn get(&self, key: &str) -> Option<String> {
        self.var(&self.env_name(key))
    }

    /// The setting by its name in the environment, as `FORGE_OIDC_ISSUER`:
    /// from the environment, else the file. Blank is not set.
    pub fn var(&self, name: &str) -> Option<String> {
        (self.env)(name)
            .or_else(|| self.values.get(name).cloned())
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }

    /// Whether `key` is on: true, 1, yes or on; false, 0, no or off; else
    /// `default` when it isn't set.
    pub fn flag(&self, key: &str, default: bool) -> Result<bool, Error> {
        let Some(value) = self.get(key) else {
            return Ok(default);
        };
        match value.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(true),
            "false" | "0" | "no" | "off" => Ok(false),
            _ => Err(self.invalid(key, "is true or false")),
        }
    }

    /// Stops when any of `keys` isn't set, naming all of them.
    pub fn require(&self, keys: &[&str]) -> Result<(), Error> {
        let missing: Vec<(String, String)> = keys
            .iter()
            .filter(|key| self.get(key).is_none())
            .map(|key| (key.to_string(), self.env_name(key)))
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(Error::Missing {
                path: self.file.clone(),
                keys: missing,
            })
        }
    }

    /// The setting `key` doesn't do: `should` says what it takes.
    pub fn invalid(&self, key: &str, should: &str) -> Error {
        Error::Invalid {
            key: key.to_string(),
            env: self.env_name(key),
            should: should.to_string(),
        }
    }

    /// `key`'s name in the environment: `oidc.issuer` is `FORGE_OIDC_ISSUER`.
    pub fn env_name(&self, key: &str) -> String {
        env_name(&self.prefix, key)
    }

    /// The file the settings came from, if any.
    pub fn file(&self) -> Option<&Path> {
        self.file.as_deref()
    }
}

fn env_name(prefix: &str, key: &str) -> String {
    format!("{prefix}_{}", key.replace('.', "_").to_uppercase())
}

/// Lists `table`'s settings as `section.key`; Err names one that isn't a
/// plain value.
fn flatten(
    section: Option<&str>,
    table: &toml::Table,
    out: &mut Vec<(String, String)>,
) -> Result<(), String> {
    for (key, value) in table {
        let key = match section {
            Some(section) => format!("{section}.{key}"),
            None => key.clone(),
        };
        match value {
            toml::Value::Table(inner) if section.is_none() => flatten(Some(&key), inner, out)?,
            toml::Value::Array(items) => {
                let items: Option<Vec<String>> = items.iter().map(plain).collect();
                out.push((key.clone(), items.ok_or(key)?.join(",")));
            }
            value => out.push((key.clone(), plain(value).ok_or(key)?)),
        }
    }
    Ok(())
}

fn plain(value: &toml::Value) -> Option<String> {
    match value {
        toml::Value::String(s) => Some(s.clone()),
        toml::Value::Integer(n) => Some(n.to_string()),
        toml::Value::Float(n) => Some(n.to_string()),
        toml::Value::Boolean(b) => Some(b.to_string()),
        _ => None,
    }
}

#[derive(Debug)]
pub enum Error {
    Read {
        path: PathBuf,
        reason: String,
    },
    Parse {
        path: PathBuf,
        reason: String,
    },
    Unknown {
        path: PathBuf,
        keys: Vec<String>,
    },
    Missing {
        path: Option<PathBuf>,
        keys: Vec<(String, String)>,
    },
    Invalid {
        key: String,
        env: String,
        should: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Read { path, reason } => write!(f, "can't read {}: {reason}", path.display()),
            Error::Parse { path, reason } => write!(f, "{} isn't valid: {reason}", path.display()),
            Error::Unknown { path, keys } => write!(
                f,
                "{} has settings that aren't known: {}",
                path.display(),
                keys.join(", ")
            ),
            Error::Missing { path, keys } => {
                let keys: Vec<String> = keys
                    .iter()
                    .map(|(key, env)| format!("{key} ({env})"))
                    .collect();
                match path {
                    Some(path) => write!(f, "{} is missing {}", path.display(), keys.join(", ")),
                    None => write!(
                        f,
                        "settings are missing: {}. Give them in a settings file (--config) or the environment",
                        keys.join(", ")
                    ),
                }
            }
            Error::Invalid { key, env, should } => {
                write!(f, "the setting {key} ({env}) {should}")
            }
        }
    }
}

impl std::error::Error for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    const KNOWN: &[&str] = &[
        "company",
        "public_url",
        "oidc.issuer",
        "oidc.client_id",
        "scim.token",
        "cookie_secure",
        "mcp.allowed_hosts",
    ];

    fn settings(
        text: &str,
        env: &'static [(&'static str, &'static str)],
    ) -> Result<Settings, Error> {
        Settings::parse(
            "forge",
            KNOWN,
            Some(PathBuf::from("/etc/forge.toml")),
            Some(text),
            Box::new(move |name| {
                env.iter()
                    .find(|(n, _)| *n == name)
                    .map(|(_, v)| v.to_string())
            }),
        )
    }

    #[test]
    fn sections_are_names_in_the_environment() {
        let s = settings(
            "company = \"Acme Inc\"\n[oidc]\nissuer = \"https://id.acme.org\"\n",
            &[],
        )
        .unwrap();
        assert_eq!(s.get("company").as_deref(), Some("Acme Inc"));
        assert_eq!(
            s.var("FORGE_OIDC_ISSUER").as_deref(),
            Some("https://id.acme.org")
        );
        assert_eq!(s.env_name("oidc.client_id"), "FORGE_OIDC_CLIENT_ID");
    }

    #[test]
    fn the_environment_wins() {
        let s = settings(
            "[oidc]\nissuer = \"https://id.acme.org\"\n",
            &[("FORGE_OIDC_ISSUER", "https://id.example.org")],
        )
        .unwrap();
        assert_eq!(
            s.get("oidc.issuer").as_deref(),
            Some("https://id.example.org")
        );
    }

    #[test]
    fn unknown_settings_stop_it() {
        let err = settings("[oidc]\nisuer = \"x\"\nwho = 1\n", &[])
            .err()
            .unwrap();
        assert_eq!(
            err.to_string(),
            "/etc/forge.toml has settings that aren't known: oidc.isuer, oidc.who"
        );
    }

    #[test]
    fn missing_settings_are_named_together() {
        let s = settings("company = \"Acme Inc\"\n", &[]).unwrap();
        let err = s
            .require(&["company", "oidc.issuer", "scim.token"])
            .unwrap_err();
        assert_eq!(
            err.to_string(),
            "/etc/forge.toml is missing oidc.issuer (FORGE_OIDC_ISSUER), scim.token (FORGE_SCIM_TOKEN)"
        );
    }

    #[test]
    fn values_and_lists() {
        let s = settings(
            "cookie_secure = false\n[mcp]\nallowed_hosts = [\"forge.acme.org\", \"localhost\"]\n",
            &[],
        )
        .unwrap();
        assert!(!s.flag("cookie_secure", true).unwrap());
        assert_eq!(
            s.get("mcp.allowed_hosts").as_deref(),
            Some("forge.acme.org,localhost")
        );
        let s = settings("", &[("FORGE_COOKIE_SECURE", "0")]).unwrap();
        assert!(!s.flag("cookie_secure", true).unwrap());
        let s = settings("", &[("FORGE_COOKIE_SECURE", "maybe")]).unwrap();
        assert!(s.flag("cookie_secure", true).is_err());
        assert!(
            settings("", &[])
                .unwrap()
                .flag("cookie_secure", true)
                .unwrap()
        );
    }

    #[test]
    fn blank_is_not_set() {
        let s = settings("company = \"  \"\n", &[]).unwrap();
        assert_eq!(s.get("company"), None);
    }

    #[test]
    fn a_file_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forge.toml");
        std::fs::write(&path, "company = \"Acme Inc\"\n").unwrap();
        let s = Settings::load("forge", KNOWN, Some(&path)).unwrap();
        assert_eq!(s.get("company").as_deref(), Some("Acme Inc"));
        assert_eq!(s.file(), Some(path.as_path()));
        let missing = Settings::load("forge", KNOWN, Some(&dir.path().join("nope.toml")));
        assert!(matches!(missing, Err(Error::Read { .. })));
    }
}
