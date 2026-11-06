//! Which version of each ACP agent is installed, against the one §20 was
//! checked with (ACP-17). An agent's ACP mode changes under it between
//! releases: `claude-agent-acp` 0.22.2 had the model call tools without
//! their schemas, and nothing in the protocol says so. Settings → ACP shows
//! the user where they stand, and runs nothing but `--version`.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::agents::AGENTS;
use crate::shellenv;

/// Long enough for a node CLI's cold start; a CLI that waits on the network
/// or a sign-in to print its version is not held for.
const PATIENCE: Duration = Duration::from_secs(5);

/// Where an installed version stands against the one checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Standing {
    /// The checked version or a later one in the same major release.
    Supported,
    /// Before the checked version: update it.
    Older,
    /// A later major release, not checked yet.
    Newer,
    /// On the PATH, but its version could not be read.
    Unknown,
    /// Not on the PATH.
    Missing,
}

#[derive(Debug, Clone, Serialize)]
pub struct AcpVersion {
    pub id: String,
    pub name: String,
    /// The ACP command, as the user would type it.
    pub command: String,
    pub path: Option<String>,
    pub version: Option<String>,
    pub checked: String,
    pub standing: Standing,
}

/// Every agent in the catalogue that runs over ACP, with its version here.
pub fn installed() -> Vec<AcpVersion> {
    AGENTS
        .iter()
        .filter_map(|a| {
            let acp = a.acp?;
            let path = shellenv::which(acp.program);
            let version = path.as_deref().and_then(read_version);
            let standing = match (&path, &version) {
                (None, _) => Standing::Missing,
                (Some(_), None) => Standing::Unknown,
                (Some(_), Some(v)) => standing(v, acp.checked),
            };
            Some(AcpVersion {
                id: a.id.to_string(),
                name: a.name.to_string(),
                command: std::iter::once(acp.program).chain(acp.args.iter().copied()).collect::<Vec<_>>().join(" "),
                path,
                version,
                checked: acp.checked.to_string(),
                standing,
            })
        })
        .collect()
}

/// `program --version`, with the login shell's environment, its first
/// dotted number. Killed if it takes longer than PATIENCE.
fn read_version(program: &str) -> Option<String> {
    let mut child = Command::new(program)
        .arg("--version")
        .env_clear()
        .envs(shellenv::user_env())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + PATIENCE;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    version_in(&out)
}

/// The first `x.y.z` (or `x.y`) in a CLI's version line: "opencode v2.2.5",
/// "GitHub Copilot CLI 1.0.100.", "0.98.0".
fn version_in(text: &str) -> Option<String> {
    text.split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .map(|w| w.trim_matches('.'))
        .find(|w| w.contains('.') && parse(w).is_some())
        .map(str::to_string)
}

fn parse(v: &str) -> Option<Vec<u64>> {
    v.split('.').map(|p| p.parse().ok()).collect()
}

fn standing(installed: &str, checked: &str) -> Standing {
    let (Some(have), Some(want)) = (parse(installed), parse(checked)) else {
        return Standing::Unknown;
    };
    if have < want {
        Standing::Older
    } else if have.first() > want.first() {
        Standing::Newer
    } else {
        Standing::Supported
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_version_is_read_from_each_clis_own_way_of_saying_it() {
        assert_eq!(version_in("0.98.0\n").as_deref(), Some("0.98.0"));
        assert_eq!(version_in("opencode v2.2.5").as_deref(), Some("2.2.5"));
        assert_eq!(
            version_in("GitHub Copilot CLI 1.0.100.\nRun 'copilot update' to check for updates.").as_deref(),
            Some("1.0.100")
        );
        assert_eq!(version_in("no version here"), None);
    }

    #[test]
    fn the_checked_version_and_later_ones_in_its_major_release_are_supported() {
        assert_eq!(standing("0.98.0", "0.98.0"), Standing::Supported);
        assert_eq!(standing("1.0.120", "1.0.100"), Standing::Supported);
        assert_eq!(standing("1.18.36", "1.18.35"), Standing::Supported);
    }

    #[test]
    fn an_older_version_is_not_supported() {
        assert_eq!(standing("0.22.2", "0.98.0"), Standing::Older);
        assert_eq!(standing("1.0.93", "1.0.100"), Standing::Older);
    }

    #[test]
    fn a_later_major_release_is_not_checked_yet() {
        assert_eq!(standing("2.2.5", "1.18.35"), Standing::Newer);
    }
}
