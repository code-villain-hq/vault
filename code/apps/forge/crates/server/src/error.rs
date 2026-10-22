use std::fmt::Display;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use forge_domain::{ApiError, ValidationError};
use forge_sync::SyncError;

/// An error response: a status and an [`ApiError`] body.
#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

/// The message, as the response would carry it (for callers outside HTTP,
/// such as the desktop app's own commands).
impl Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl AppError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        AppError {
            status,
            code,
            message: message.into(),
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid", message)
    }

    /// Not signed in, or the session has ended. Clients go to the sign-in screen.
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNAUTHORIZED, "unauthorized", message)
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::new(StatusCode::FORBIDDEN, "forbidden", message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, "not_found", message)
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::new(StatusCode::CONFLICT, "conflict", message)
    }

    /// Logs the cause and tells the client only that something went wrong.
    pub fn internal(cause: impl Display) -> Self {
        tracing::error!(error = %cause, "internal error");
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "something went wrong",
        )
    }
}

impl From<SyncError> for AppError {
    fn from(e: SyncError) -> Self {
        match e {
            SyncError::Invalid(message) => AppError::invalid(message),
            SyncError::NotFound(what) => AppError::not_found(format!("{what} not found")),
            SyncError::Forbidden(message) => AppError::forbidden(message),
            SyncError::Conflict(message) => AppError::conflict(message),
            SyncError::Database(db) => AppError::internal(db),
        }
    }
}

impl From<ValidationError> for AppError {
    fn from(e: ValidationError) -> Self {
        AppError::invalid(e.0)
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::internal(e)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let body = ApiError {
            code: self.code.into(),
            message: self.message,
        };
        (self.status, Json(body)).into_response()
    }
}
