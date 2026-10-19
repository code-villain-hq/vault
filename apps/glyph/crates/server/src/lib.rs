//! Glyph's server: the HTTP API, the sync feed and accounts, the same as
//! Forge's, plus translating with local models. `main.rs` runs it; tests
//! drive [`router`] directly.

pub mod accounts;
pub mod auth;
pub mod config;
pub mod error;
pub mod invites;
pub mod limits;
pub mod members;
pub mod models;
pub mod notify;
pub mod routes;
pub mod state;
pub mod translate;
pub mod ws;

pub use routes::{app, router};
pub use state::AppState;
