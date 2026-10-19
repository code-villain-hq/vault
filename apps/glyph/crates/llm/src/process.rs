//! Model servers the Glyph server starts itself, such as `llama-server`,
//! with the last lines they printed, for when one fails to come up.

use std::collections::{HashMap, VecDeque};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Command;

/// Lines kept from each server's output.
const LINES: usize = 200;

/// A started server, as the API shows it.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub pid: Option<u32>,
    /// Seconds since 1970.
    pub started_at: u64,
    /// How it ended, once it has.
    pub exited: Option<String>,
    pub recent_output: Vec<String>,
}

struct Process {
    pid: Option<u32>,
    started_at: u64,
    exited: Mutex<Option<String>>,
    output: Mutex<VecDeque<String>>,
}

impl Process {
    fn status(&self) -> Status {
        Status {
            pid: self.pid,
            started_at: self.started_at,
            exited: self.exited.lock().unwrap().clone(),
            recent_output: self.output.lock().unwrap().iter().cloned().collect(),
        }
    }

    fn running(&self) -> bool {
        self.exited.lock().unwrap().is_none()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("It's already running.")]
    AlreadyRunning,
    #[error("It isn't running from here.")]
    NotRunning,
    #[error("couldn't start {binary}: {source}")]
    Spawn {
        binary: String,
        source: std::io::Error,
    },
}

/// The servers started, by model name.
#[derive(Clone, Default)]
pub struct Processes {
    running: Arc<Mutex<HashMap<String, Arc<Process>>>>,
}

impl Processes {
    pub fn new() -> Processes {
        Processes::default()
    }

    /// Starts `binary args…` for the model `name` and returns at once; the
    /// model's `/health` says when it's ready.
    pub fn start(&self, name: &str, binary: &str, args: &[String]) -> Result<Status, ProcessError> {
        let mut running = self.running.lock().unwrap();
        if running.get(name).is_some_and(|p| p.running()) {
            return Err(ProcessError::AlreadyRunning);
        }
        let mut command = Command::new(binary);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // Its own process group, so stopping it stops what it started too.
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command.spawn().map_err(|source| ProcessError::Spawn {
            binary: binary.to_string(),
            source,
        })?;

        let process = Arc::new(Process {
            pid: child.id(),
            started_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or_default(),
            exited: Mutex::new(None),
            output: Mutex::new(VecDeque::new()),
        });
        if let Some(out) = child.stdout.take() {
            tokio::spawn(keep_lines(out, process.clone()));
        }
        if let Some(err) = child.stderr.take() {
            tokio::spawn(keep_lines(err, process.clone()));
        }
        let watched = process.clone();
        tokio::spawn(async move {
            let ended = match child.wait().await {
                Ok(status) => status.to_string(),
                Err(e) => e.to_string(),
            };
            *watched.exited.lock().unwrap() = Some(ended);
        });

        let status = process.status();
        running.insert(name.to_string(), process);
        Ok(status)
    }

    pub fn status(&self, name: &str) -> Option<Status> {
        self.running.lock().unwrap().get(name).map(|p| p.status())
    }

    /// Asks the server to stop, and kills it if it hasn't after `grace`.
    pub async fn stop(&self, name: &str, grace: Duration) -> Result<(), ProcessError> {
        let process = self
            .running
            .lock()
            .unwrap()
            .remove(name)
            .ok_or(ProcessError::NotRunning)?;
        let Some(pid) = process.pid.filter(|_| process.running()) else {
            return Ok(());
        };
        signal("-TERM", pid).await;
        let deadline = tokio::time::Instant::now() + grace;
        while process.running() && tokio::time::Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if process.running() {
            signal("-KILL", pid).await;
        }
        Ok(())
    }

    /// Stops every server, for shutting down.
    pub async fn stop_all(&self, grace: Duration) {
        let names: Vec<String> = self.running.lock().unwrap().keys().cloned().collect();
        for name in names {
            let _ = self.stop(&name, grace).await;
        }
    }
}

/// Signals the process group `pid` leads, with `kill`, which keeps this
/// crate free of unsafe code.
async fn signal(which: &str, pid: u32) {
    #[cfg(unix)]
    let _ = Command::new("kill")
        .args([which, "--", &format!("-{pid}")])
        .status()
        .await;
    #[cfg(not(unix))]
    let _ = Command::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .status()
        .await
        .map(|_| which);
}

async fn keep_lines(stream: impl AsyncRead + Unpin, process: Arc<Process>) {
    let mut lines = BufReader::new(stream).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let mut output = process.output.lock().unwrap();
        output.push_back(line);
        if output.len() > LINES {
            output.pop_front();
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_server_starts_prints_and_stops() {
        let processes = Processes::new();
        let script = vec!["-c".to_string(), "echo ready; sleep 30".to_string()];
        let started = processes.start("m", "sh", &script).unwrap();
        assert!(started.pid.is_some());
        assert!(matches!(
            processes.start("m", "sh", &script),
            Err(ProcessError::AlreadyRunning)
        ));

        for _ in 0..50 {
            if !processes.status("m").unwrap().recent_output.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(processes.status("m").unwrap().recent_output, ["ready"]);

        processes.stop("m", Duration::from_secs(2)).await.unwrap();
        assert!(processes.status("m").is_none());
        assert!(matches!(
            processes.stop("m", Duration::from_secs(1)).await,
            Err(ProcessError::NotRunning)
        ));
    }

    #[tokio::test]
    async fn a_missing_binary_says_so() {
        let processes = Processes::new();
        let missing = processes.start("m", "/nonexistent/llama-server", &[]);
        assert!(matches!(missing, Err(ProcessError::Spawn { .. })));
    }
}
