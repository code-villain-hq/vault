//! What a studio desktop app needs to run on its own, with no server
//! elsewhere: its own Postgres ([`LocalPostgres`]) and its own server on
//! loopback ([`LocalServer`]). The app brings the server; this crate starts
//! and stops it.

mod postgres;
mod server;

pub use postgres::LocalPostgres;
pub use server::{LocalServer, Shutdown};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Postgres: {0}")]
    Postgres(#[from] postgresql_embedded::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
