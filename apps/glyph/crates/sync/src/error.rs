use glyph_domain::ValidationError;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// The input breaks a rule: a bad field, an empty key, an unknown language.
    #[error("{0}")]
    Invalid(String),
    #[error("{0} not found")]
    NotFound(String),
    /// The actor isn't allowed to make the change, such as a member deleting
    /// a language. The message says who may, in plain words.
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
            if let Some(friendly) = db.constraint().and_then(plain_words) {
                return match db.code().as_deref() {
                    Some("23505") => SyncError::Conflict(friendly.into()),
                    _ => SyncError::Invalid(friendly.into()),
                };
            }
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

/// What a broken constraint means, for the ones people run into.
fn plain_words(constraint: &str) -> Option<&'static str> {
    Some(match constraint {
        "languages_workspace_id_code_key" => "That language is already in the workspace.",
        "languages_one_source_idx" => "Only one language can be the source.",
        "translations_workspace_id_lang_namespace_key_key" => {
            "That key already exists in this namespace."
        }
        "glossary_entries_term_idx" => "There's already a glossary entry for that term.",
        "language_pairs_workspace_id_source_lang_target_lang_key" => {
            "Those two languages already have settings."
        }
        c if c.ends_with("_lang_fkey") => "Add that language to the workspace first.",
        _ => return None,
    })
}
