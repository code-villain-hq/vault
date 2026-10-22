//! Layer's server: workspaces and their members, and the sync feed of a
//! workspace's shared setup (repositories, connections, log sources, agent
//! models and repo notes). Agents and terminals run only in the desktop app;
//! the server keeps what a team sets up together. [`serve`] runs it, from
//! `main.rs` or the desktop app; tests drive [`router`] directly.

pub mod accounts;
pub mod auth;
pub mod company;
pub mod config;
pub mod error;
pub mod groups;
pub mod local;
pub mod members;
pub mod oidc;
pub mod routes;
mod serve;
pub mod state;
pub mod ws;

pub use routes::{Web, app, router};
pub use serve::serve;
pub use state::AppState;
