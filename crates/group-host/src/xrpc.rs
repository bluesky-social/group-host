//! XRPC's failures, as atproto's servers answer them: an HTTP status and
//! `{error, message}`.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XrpcError {
    pub status: StatusCode,
    /// The failure's name, as the method's lexicon lists it.
    pub error: &'static str,
    pub message: String,
}

impl XrpcError {
    pub fn new(status: StatusCode, error: &'static str, message: impl Into<String>) -> Self {
        XrpcError {
            status,
            error,
            message: message.into(),
        }
    }

    /// What went wrong inside the host, said as it is: the database, or
    /// a server it asked.
    pub fn internal(err: impl std::fmt::Display) -> Self {
        XrpcError::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "InternalServerError",
            err.to_string(),
        )
    }

    /// A call this host has no method for.
    pub fn not_implemented(nsid: &str) -> Self {
        XrpcError::new(
            StatusCode::NOT_IMPLEMENTED,
            "MethodNotImplemented",
            format!("{nsid} is not a method this host answers"),
        )
    }
}

impl IntoResponse for XrpcError {
    fn into_response(self) -> Response {
        let body = json!({ "error": self.error, "message": self.message });
        (self.status, Json(body)).into_response()
    }
}
