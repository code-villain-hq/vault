//! Glyph's server: the HTTP API, the sync feed, sign-in and workspaces, the
//! same as Forge's, plus translating with local models. [`serve`] runs it, from
//! `main.rs` or the desktop app; tests drive [`router`] directly.

pub mod accounts;
pub mod auth;
pub mod company;
pub mod config;
pub mod error;
pub mod groups;
pub mod local;
pub mod members;
pub mod models;
pub mod oidc;
pub mod routes;
mod serve;
pub mod state;
pub mod translate;
pub mod ws;

pub use routes::{app, router};
pub use serve::serve;
pub use state::AppState;
