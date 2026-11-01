//! The desktop app's half: Claude Code on this Mac for a server's
//! workspace. The server hands the question over ([`crate::handed`]),
//! Claude Code answers it here with the server's tools at its MCP
//! endpoint, and the web app says what it made of it. It runs as for the
//! workspaces on this Mac: in an empty folder of its own, as you're signed
//! in to it, with no key.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::acp::{Adapter, App, Kind};
use crate::handed::{self, Answered, HandedQuestion};
use crate::info::{ClaudeCodeInfo, how_it_is};

/// Held by the desktop app.
pub struct Here {
    app: App,
    folder: PathBuf,
    /// The app's MCP endpoint on a server: `/api/ai/mcp`.
    mcp_path: &'static str,
    /// One adapter per server, so a check that found it ready holds.
    adapters: Mutex<HashMap<String, Adapter>>,
}

impl Here {
    pub fn new(app: App, folder: impl Into<PathBuf>, mcp_path: &'static str) -> Self {
        Here {
            app,
            folder: folder.into(),
            mcp_path,
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
            Adapter::new(
                self.app,
                Kind::ClaudeCode,
                &self.folder,
                format!("{origin}{}", self.mcp_path),
            )
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
