//! Raven's server: accounts, the health checks it makes, what Forge and
//! Glyph send it over OpenTelemetry, and the pages that show it all. It
//! watches itself too. `main.rs` runs it; tests drive [`router`] directly.

pub mod accounts;
pub mod auth;
pub mod config;
pub mod error;
pub mod health;
pub mod limits;
pub mod metrics;
pub mod oidc;
pub mod otlp;
pub mod routes;
pub mod state;
pub mod telemetry;

pub use routes::{app, router};
pub use state::AppState;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../../migrations");
