//! Forge's server: the HTTP API, the sync feed and accounts. Its specs live
//! in Forge ("Architecture overview" and each service's). `main.rs` runs it; tests drive [`router`] directly.

pub mod accounts;
pub mod agents;
pub mod attachments;
pub mod auth;
pub mod config;
pub mod drift;
pub mod error;
pub mod forgejo;
pub mod github;
pub mod invites;
pub mod limits;
pub mod mcp;
pub mod members;
pub mod notify;
pub mod routes;
pub mod runners;
pub mod state;
pub mod teams;
pub mod ws;

pub use routes::{app, router};
pub use state::AppState;
