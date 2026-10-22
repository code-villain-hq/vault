//! dev-id: a stand-in for a company's account, so every app runs locally as
//! it does for a company. It signs people in over OpenID Connect, as Pocket
//! ID does, at http://127.0.0.1:7280, and sends each app its people and
//! groups over SCIM, from `people.toml`. Development only: anyone can sign
//! in as anyone, and its tokens are signed with a key everyone knows.
//!
//! `dev-id [--people <file>] [--addr <host:port>]`

mod oidc;
mod scim;

use std::path::PathBuf;
use std::sync::Arc;

use serde::Deserialize;

/// The apps, their clients' secret and SCIM token (both `dev`), and where
/// their local servers listen.
pub const APPS: &[(&str, &str, &str)] = &[
    ("forge", "Forge", "http://127.0.0.1:7380"),
    ("glyph", "Glyph", "http://127.0.0.1:7480"),
    ("raven", "Raven", "http://127.0.0.1:7580"),
    ("layer", "Layer", "http://127.0.0.1:7680"),
];

pub const SECRET: &str = "dev";

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct People {
    pub company: String,
    #[serde(default)]
    pub person: Vec<Person>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub email: String,
    #[serde(default)]
    pub groups: Vec<String>,
}

impl People {
    fn read(path: &std::path::Path) -> anyhow::Result<People> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("can't read {}: {e}", path.display()))?;
        toml::from_str(&text).map_err(|e| anyhow::anyhow!("{} isn't valid: {e}", path.display()))
    }

    /// The people an app gets: those in one of its groups.
    pub fn of(&self, app: &str) -> Vec<&Person> {
        let prefix = format!("{app}-");
        self.person
            .iter()
            .filter(|p| p.groups.iter().any(|g| g.starts_with(&prefix)))
            .collect()
    }

    pub fn find(&self, who: &str) -> Option<&Person> {
        self.person
            .iter()
            .find(|p| p.id == who || p.email.eq_ignore_ascii_case(who))
    }
}

pub struct DevId {
    pub issuer: String,
    pub people_file: PathBuf,
}

impl DevId {
    /// The people as the file says now, so edits apply at once.
    pub fn people(&self) -> anyhow::Result<People> {
        People::read(&self.people_file)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut people_file = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/people.toml"));
    let mut addr = "127.0.0.1:7280".to_string();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match (arg.as_str(), args.next()) {
            ("--people", Some(file)) => people_file = file.into(),
            ("--addr", Some(a)) => addr = a,
            _ => {
                eprintln!("usage: dev-id [--people <file>] [--addr <host:port>]");
                std::process::exit(2);
            }
        }
    }
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => listener,
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            println!("dev-id is already running at http://{addr}");
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };
    let dev = Arc::new(DevId {
        issuer: format!("http://{addr}"),
        people_file,
    });
    let people = dev.people()?;
    println!(
        "dev-id: signing in {} for {} people at {}",
        people.company,
        people.person.len(),
        dev.issuer
    );
    tokio::spawn(scim::keep_pushing(dev.clone()));
    axum::serve(listener, oidc::router(dev)).await?;
    Ok(())
}
