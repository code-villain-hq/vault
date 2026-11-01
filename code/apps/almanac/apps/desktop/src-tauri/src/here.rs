//! Claude Code on this Mac for a server's workspace: it answers first,
//! wherever the workspace is (`studio/design/behaviour.md`, "Claude"). The
//! server hands the question over (`almanac_server::handed`), Claude Code
//! answers it here with the server's tools at its `/api/mcp`, and the web
//! app says what it made of it. It runs as for the workspaces on this Mac:
//! in an empty folder of its own, as you're signed in to it, with no key.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use almanac_domain::{Answered, ClaudeCodeInfo, HandedQuestion};
use almanac_server::acp::{Adapter, Kind};
use almanac_server::handed;
use almanac_server::keys::how_it_is;

/// Held by the app.
pub struct Here {
    folder: PathBuf,
    /// One adapter per server, so a check that found it ready holds.
    adapters: Mutex<HashMap<String, Adapter>>,
}

impl Here {
    pub fn new(folder: PathBuf) -> Self {
        Here {
            folder,
            adapters: Mutex::default(),
        }
    }

    /// Claude Code, with the tools of the server at `origin`.
    fn adapter(&self, origin: &str) -> Result<Adapter, String> {
        let origin = origin.trim_end_matches('/');
        if !origin.starts_with("https://") && !origin.starts_with("http://") {
            return Err(format!("{origin} isn't a server's address."));
        }
        let mut adapters = self.adapters.lock().map_err(|e| e.to_string())?;
        let adapter = adapters.entry(origin.to_string()).or_insert_with(|| {
            Adapter::new(Kind::ClaudeCode, &self.folder, format!("{origin}/api/mcp"))
        });
        Ok(adapter.clone())
    }

    /// Whether Claude Code can answer here, as Settings → Claude shows it.
    pub async fn check(&self, origin: &str) -> Result<ClaudeCodeInfo, String> {
        Ok(how_it_is(&self.adapter(origin)?).await)
    }

    /// Answers the question the server at `origin` handed over, each piece
    /// to `piece` as it's written, and says what came of it.
    pub async fn answer(
        &self,
        origin: &str,
        question: HandedQuestion,
        piece: impl FnMut(&str) + Send,
    ) -> Result<Answered, String> {
        let adapter = self.adapter(origin)?;
        Ok(handed::answer(&adapter, question, piece).await)
    }
}
