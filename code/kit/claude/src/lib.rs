//! Claude Code on the asker's Mac, for every studio app: what a person asks
//! Claude in the desktop app is answered first by Claude Code on their Mac,
//! wherever the workspace is (`studio/design/behaviour.md`, "Claude").
//!
//! - [`acp`]: Claude Code (or ChatGPT) on this Mac, started per question
//!   over the Agent Client Protocol, with the app's tools and nothing else.
//! - [`questions`]: the token each question opens, which the app's MCP
//!   endpoint and `/api/answered` know it by.
//! - [`mcp`]: the app's tools for one question, as Claude Code calls them.
//! - [`handed`]: a server's question handed to the asker's Mac, and what
//!   the Mac made of it.
//! - [`here`]: the desktop app's half, which answers what a server hands it.
//! - [`info`]: whether Claude Code can answer, as Settings shows it.

pub mod acp;
pub mod handed;
pub mod here;
pub mod info;
mod login_env;
pub mod mcp;
pub mod questions;

pub use acp::{Adapter, Answer, App, Failure, Image, Kind, Question};
pub use handed::{Answered, HandedPhoto, HandedQuestion, NotAnswered};
pub use here::Here;
pub use info::{ClaudeCodeInfo, how_it_is};
pub use questions::{Asked, Open, Questions, bearer};
