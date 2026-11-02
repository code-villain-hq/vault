//! Postrider's server: you and your workspace, the mail accounts and the
//! calendar server it reads, the unified Inbox it keeps from them over IMAP,
//! the day it reads over CalDAV, and the API its web app reads.
//! [`serve`] runs it, from `main.rs` or the desktop app; tests drive
//! [`router`] directly.

pub mod accounts;
pub mod archive;
pub mod ask;
pub mod auth;
pub mod caldav;
pub mod connect;
pub mod error;
pub mod files;
pub mod folders;
pub mod imap;
pub mod local;
pub mod mail;
pub mod message;
pub mod newsletters;
pub mod preview;
pub mod reading;
pub mod reply;
pub mod rewrite;
pub mod routes;
pub mod rules;
pub mod secrets;
mod serve;
pub mod sieve;
pub mod state;
pub mod ws;

pub use routes::{app, router};
pub use serve::serve;
pub use state::AppState;
