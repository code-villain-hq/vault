use forge_domain::ValidationError;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// The input breaks a rule: a bad field, a missing title, an unknown reference.
    #[error("{0}")]
    Invalid(String),
    #[error("{0} not found")]
    NotFound(String),
    /// The actor isn't allowed to make the change, such as a member deleting
    /// a spec. The message says who may, in plain words.
    #[error("{0}")]
    Forbidden(String),
    /// The change collides with existing data, such as a record id already in use.
    #[error("{0}")]
    Conflict(String),
    #[error(transparent)]
    Database(sqlx::Error),
}

impl From<ValidationError> for SyncError {
    fn from(e: ValidationError) -> Self {
        SyncError::Invalid(e.0)
    }
}

impl From<sqlx::Error> for SyncError {
    /// Turns constraint violations into errors the client can act on.
    fn from(e: sqlx::Error) -> Self {
        if let sqlx::Error::Database(db) = &e {
            let message = db.message().to_string();
            match db.code().as_deref() {
                // unique_violation
                Some("23505") => return SyncError::Conflict(message),
                // not_null, foreign_key, check, invalid_text_representation,
                // numeric_value_out_of_range, string_data_right_truncation
                Some("23502" | "23503" | "23514" | "22P02" | "22003" | "22001") => {
                    return SyncError::Invalid(message);
                }
                _ => {}
            }
        }
        SyncError::Database(e)
    }
}
