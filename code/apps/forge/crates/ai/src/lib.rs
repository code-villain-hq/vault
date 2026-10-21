//! Forge's AI plumbing, independent of the tracker: a provider for the
//! Claude Messages API (and a scripted one for tests), the tool-use loop,
//! and read-only access to a repository's tracked files.
//!
//! What the AI does with these (drift checks) lives in the server, next to
//! the data it reads and the sync engine it writes through.

mod agent;
mod provider;
mod repo;

pub use agent::{Agent, AgentRun, Stop, ToolHandler, ToolOutput, Usage};
pub use provider::{AiError, ClaudeProvider, ModelProvider, ScriptedProvider};
pub use repo::{Repo, RepoError, matches_any};
