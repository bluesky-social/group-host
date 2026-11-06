//! XRPC's failures, as atproto's servers answer them: an HTTP status and
//! `{error, message}`.

use axum::Json;
use axum::body::{Body, Bytes};
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
    /// The failure as JSON, and kept with the answer, for the line logged
    /// of the request to say why.
    fn into_response(self) -> Response {
        let body = json!({ "error": self.error, "message": self.message });
        let mut res = (self.status, Json(body)).into_response();
        res.extensions_mut().insert(self);
        res
    }
}

/// A request's body, at most `limit` bytes, read once its caller is known
/// to be let send one: a longer body, or one cut off, is refused as
/// `too_large`, as the method's lexicon names it.
pub async fn read_body(
    body: Body,
    limit: u64,
    too_large: &'static str,
) -> Result<Bytes, XrpcError> {
    let most = usize::try_from(limit).unwrap_or(usize::MAX);
    axum::body::to_bytes(body, most).await.map_err(|e| {
        let said = format!("a body of {limit} bytes at most, read whole: {e}");
        XrpcError::new(StatusCode::PAYLOAD_TOO_LARGE, too_large, said)
    })
}
