use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Application-wide error type. Every handler returns `Result<T, AppError>`.
#[derive(Debug)]
pub enum AppError {
    /// Resource not found (404).
    NotFound(String),
    /// Missing or invalid authentication (401/403).
    Unauthorized(String),
    /// Invalid input from the client (400).
    BadRequest(String),
    /// Rate limit exceeded (429).
    TooManyRequests(String),
    /// Resource conflict, e.g. duplicate creation (409).
    Conflict(String),
    /// Action prohibited by business rules (403).
    Forbidden(String),
    /// Internal server error (500).
    Internal(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(msg) => write!(f, "{msg}"),
            Self::Unauthorized(msg) => write!(f, "{msg}"),
            Self::BadRequest(msg) => write!(f, "{msg}"),
            Self::TooManyRequests(msg) => write!(f, "{msg}"),
            Self::Conflict(msg) => write!(f, "{msg}"),
            Self::Forbidden(msg) => write!(f, "{msg}"),
            Self::Internal(msg) => write!(f, "{msg}"),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            Self::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg.clone()),
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            Self::TooManyRequests(msg) => (StatusCode::TOO_MANY_REQUESTS, msg.clone()),
            Self::Conflict(msg) => (StatusCode::CONFLICT, msg.clone()),
            Self::Forbidden(msg) => (StatusCode::FORBIDDEN, msg.clone()),
            Self::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
        };

        (status, Json(json!({ "error": message }))).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        tracing::error!("Database error: {err}");
        Self::Internal("Database error".to_string())
    }
}
