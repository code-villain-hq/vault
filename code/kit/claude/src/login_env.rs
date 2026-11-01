//! The environment Claude Code runs in. An app opened from Finder gets a
//! bare PATH, which has no `claude-agent-acp`, so the login shell is asked
//! what it really has, as Layer does (`code/apps/layer`, `shellenv.rs`).

use std::collections::HashMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

static ENV: OnceLock<HashMap<String, String>> = OnceLock::new();

/// Printed just before the environment, so whatever the startup files print
/// first isn't read as part of it.
const MARK: &str = "__CODE_VILLAIN_ENV__";

/// How long the startup files get. One that waits for input would
/// otherwise hold every answer.
const SHELL_TIMEOUT: Duration = Duration::from_secs(10);

/// Not the user's environment but where the scrape ran.
fn inherited_by_accident(key: &str) -> bool {
    matches!(key, "PWD" | "OLDPWD" | "SHLVL" | "_")
}

/// What a Claude Code session sets for what it starts. The app carries them
/// when it was opened from one, and Claude Code would then take itself for
/// that session's subagent. Dropped from the app's own environment only, so
/// one the user exports in their shell survives.
fn from_a_claude_session(key: &str) -> bool {
    matches!(key, "CLAUDECODE" | "CLAUDE_PID" | "CLAUDE_EFFORT")
        || key.starts_with("CLAUDE_CODE_")
        || key.starts_with("CLAUDE_AGENT_SDK_")
        || key.starts_with("CLAUDE_PREVIEW_")
}

fn ask_login_shell(own: &HashMap<String, String>) -> Option<Vec<u8>> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let mut child = Command::new(shell)
        .args(["-ilc", &format!("printf '\\0{MARK}\\0'; env -0")])
        .env_clear()
        .envs(own)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    // Read beside the wait: something the startup files left running can
    // hold the pipe open after the shell has gone.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut out = Vec::new();
        let _ = stdout.read_to_end(&mut out);
        let _ = tx.send(out);
    });
    let deadline = Instant::now() + SHELL_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                tracing::warn!(
                    "the login shell didn't answer in time; using the app's own environment"
                );
                return None;
            }
        }
    };
    if !status.success() {
        return None;
    }
    rx.recv_timeout(Duration::from_secs(1)).ok()
}

/// The records of `env -0` after the marker.
fn parse_env(out: &[u8]) -> Vec<(String, String)> {
    let mut records = out.split(|b| *b == 0);
    if !records.any(|r| r == MARK.as_bytes()) {
        return Vec::new();
    }
    records
        .filter(|r| !r.is_empty())
        .filter_map(|r| {
            let s = String::from_utf8_lossy(r);
            let (k, v) = s.split_once('=')?;
            Some((k.to_string(), v.to_string()))
        })
        .collect()
}

/// The login shell's environment over the app's own, asked once. It waits
/// on the shell: call it off the async runtime.
pub fn user_env() -> &'static HashMap<String, String> {
    ENV.get_or_init(|| {
        let mut env: HashMap<String, String> = std::env::vars()
            .filter(|(k, _)| !from_a_claude_session(k))
            .collect();
        if let Some(out) = ask_login_shell(&env) {
            env.extend(parse_env(&out));
        }
        env.retain(|k, _| !inherited_by_accident(k) && !from_a_claude_session(k));
        env
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_the_startup_files_print_first_is_not_read_as_the_environment() {
        let out = format!("hello\0{MARK}\0PATH=/opt/bin:/usr/bin\0HOME=/Users/me\0");
        assert_eq!(
            parse_env(out.as_bytes()),
            vec![
                ("PATH".to_string(), "/opt/bin:/usr/bin".to_string()),
                ("HOME".to_string(), "/Users/me".to_string()),
            ]
        );
        assert!(parse_env(b"PATH=/usr/bin\0").is_empty());
    }

    #[test]
    fn a_claude_code_sessions_own_variables_stay_behind() {
        assert!(from_a_claude_session("CLAUDECODE"));
        assert!(from_a_claude_session("CLAUDE_CODE_ENTRYPOINT"));
        assert!(!from_a_claude_session("CLAUDE_CONFIG_DIR"));
    }
}
