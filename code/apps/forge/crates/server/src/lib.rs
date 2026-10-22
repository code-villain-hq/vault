//! Forge's server: the HTTP API, the sync feed, sign-in and workspaces. Its specs live
//! in Forge ("Architecture overview" and each service's). [`serve`] runs it, from `main.rs` or the desktop app; tests drive [`router`]
//! directly.

pub mod accounts;
pub mod agents;
pub mod attachments;
pub mod auth;
pub mod company;
pub mod config;
pub mod drift;
pub mod error;
pub mod forgejo;
pub mod github;
pub mod groups;
pub mod local;
pub mod mcp;
pub mod members;
pub mod oidc;
pub mod routes;
pub mod runners;
mod serve;
pub mod state;
pub mod teams;
pub mod ws;

pub use routes::{app, router};
pub use serve::serve;
pub use state::AppState;
