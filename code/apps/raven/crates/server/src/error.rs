use std::fmt::Display;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// An error response: a status and an [`ApiError`] body.
#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    code: &'static str,
    pub message: String,
    /// What really went wrong, for Raven's own record of the call; the
    /// client only hears `message`.
    cause: Option<String>,
}

/// The body of every error response.
#[derive(Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

/// Rides on an error response so the call recorder can say what went wrong.
#[derive(Clone, Debug)]
pub struct ErrorNote(pub String);

impl AppError {
    fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        AppError {
            status,
            code,
            message: message.into(),
            cause: None,
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

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported", message)
    }

    /// Refused for now, after too many wrong passwords.
    pub fn too_many(wait: std::time::Duration) -> Self {
        Self::try_again_later("too many wrong passwords", wait)
    }

    /// Refused for now, after too many sign-ups from the same place.
    pub fn too_many_signups(wait: std::time::Duration) -> Self {
        Self::try_again_later("too many sign-ups from here", wait)
    }

    fn try_again_later(what: &str, wait: std::time::Duration) -> Self {
        let minutes = wait.as_secs().div_ceil(60).max(1);
        let unit = if minutes == 1 { "minute" } else { "minutes" };
        Self::new(
            StatusCode::TOO_MANY_REQUESTS,
            "too_many_attempts",
            format!("{what}; try again in {minutes} {unit}"),
        )
    }

    /// Logs the cause and tells the client only that something went wrong.
    pub fn internal(cause: impl Display) -> Self {
        let cause = cause.to_string();
        tracing::error!(error = %cause, "internal error");
        AppError {
            cause: Some(cause),
            ..Self::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "something went wrong",
            )
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::internal(e)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let note = ErrorNote(self.cause.unwrap_or_else(|| self.message.clone()));
        let body = ApiError {
            code: self.code.into(),
            message: self.message,
        };
        let mut response = (self.status, Json(body)).into_response();
        response.extensions_mut().insert(note);
        response
    }
}
