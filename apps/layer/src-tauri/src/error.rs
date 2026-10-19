use serde::{Serialize, Serializer};

/// Every failure that can cross the Rust -> frontend boundary.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("git: {0}")]
    Git(String),

    #[error("pty: {0}")]
    Pty(String),

    #[error("io: {0}")]
    Io(#[from] std::io::Error),

    #[error("http: {0}")]
    Http(#[from] reqwest::Error),

    #[error("json: {0}")]
    Json(#[from] serde_json::Error),

    #[error("keychain: {0}")]
    Keyring(#[from] keyring::Error),

    #[error("not found: {0}")]
    NotFound(String),

    /// The user has not connected this integration yet.
    #[error("{0} is not configured")]
    NotConfigured(&'static str),

    /// The service answered 401 to the token saved for it: expired, revoked,
    /// or replaced. The frontend knows this text (`TOKEN_REJECTED`) and offers
    /// to reconnect instead of showing it per repo (PR-12).
    #[error("{0} no longer accepts the saved token")]
    TokenRejected(&'static str),

    #[error("{0}")]
    Other(String),
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
