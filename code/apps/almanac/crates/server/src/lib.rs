//! Almanac's server: workspaces and their members, the sync feed of what
//! each keeps (topics, their pages, talks and reminders), the talks with
//! Claude, and telling the Mac when a reminder comes due.
//! [`serve`] runs it, from `main.rs` or the desktop app; tests drive
//! [`router`] directly.

pub mod accounts;
pub mod acp;
pub mod auth;
pub mod claude;
pub mod company;
pub mod config;
pub mod convert;
pub mod error;
pub mod groups;
pub mod kept;
pub mod keys;
pub mod layout;
pub mod local;
mod login_env;
pub mod mcp;
pub mod members;
pub mod oidc;
pub mod own;
pub mod page;
pub mod photos;
pub mod reminders;
pub mod routes;
mod serve;
pub mod state;
pub mod talks;
pub mod topics;
pub mod ws;

pub use routes::{app, router};
pub use serve::serve;
pub use state::AppState;
