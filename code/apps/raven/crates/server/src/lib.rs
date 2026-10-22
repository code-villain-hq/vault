//! Raven's server: workspaces and their members, the sync feed of the apps
//! each watches, the health checks it makes, what the apps send it over
//! OpenTelemetry, and the pages that show it all. It watches itself too.
//! [`serve`] runs it, from `main.rs` or the desktop app; tests drive
//! [`router`] directly.

pub mod accounts;
pub mod apps;
pub mod auth;
pub mod config;
pub mod error;
pub mod groups;
pub mod health;
pub mod invites;
pub mod limits;
pub mod local;
pub mod members;
pub mod metrics;
pub mod oidc;
pub mod otlp;
pub mod routes;
mod serve;
pub mod state;
pub mod telemetry;
pub mod ws;

pub use routes::{app, router};
pub use serve::serve;
pub use state::AppState;
