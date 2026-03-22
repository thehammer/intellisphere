use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum SphereError {
    #[error("Authentication failed: {0}")]
    AuthError(String),

    #[error("Rate limit exceeded")]
    RateLimited,

    #[error("Request rejected by filter '{filter}': {reason}")]
    FilterRejected { filter: String, reason: String },

    #[error("Core unavailable: {0}")]
    CoreUnavailable(String),

    #[error("Core error: {0}")]
    CoreError(String),

    #[error("Tool error: {0}")]
    ToolError(String),

    #[error("Outbound filter rejected: {0}")]
    OutboundRejected(String),

    #[error("Policy violation: {0}")]
    PolicyViolation(String),

    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    code: String,
    message: String,
}

impl IntoResponse for SphereError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            SphereError::AuthError(_) => (StatusCode::UNAUTHORIZED, "AUTH_FAILED"),
            SphereError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "RATE_LIMITED"),
            SphereError::FilterRejected { .. } => (StatusCode::BAD_REQUEST, "FILTER_REJECTED"),
            SphereError::CoreUnavailable(_) => {
                (StatusCode::SERVICE_UNAVAILABLE, "CORE_UNAVAILABLE")
            }
            SphereError::CoreError(_) => (StatusCode::BAD_GATEWAY, "CORE_ERROR"),
            SphereError::ToolError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "TOOL_ERROR"),
            SphereError::OutboundRejected(_) => {
                (StatusCode::UNPROCESSABLE_ENTITY, "OUTBOUND_REJECTED")
            }
            SphereError::PolicyViolation(_) => (StatusCode::FORBIDDEN, "POLICY_VIOLATION"),
            SphereError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "INTERNAL_ERROR"),
        };

        let body = ErrorResponse {
            error: code.to_string(),
            code: code.to_string(),
            message: self.to_string(),
        };

        (status, axum::Json(body)).into_response()
    }
}
