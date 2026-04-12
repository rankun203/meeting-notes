use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::json;

/// Unified error type for the services layer.
///
/// Each variant maps cleanly to an HTTP status code (for the axum/REST transport)
/// and is also `Serialize` so Tauri commands can return it via `Result<T, ServiceError>`.
#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[serde(tag = "kind", content = "message", rename_all = "snake_case")]
pub enum ServiceError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("upstream error: {0}")]
    BadGateway(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl ServiceError {
    pub fn status(&self) -> StatusCode {
        match self {
            ServiceError::NotFound(_) => StatusCode::NOT_FOUND,
            ServiceError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ServiceError::Conflict(_) => StatusCode::CONFLICT,
            ServiceError::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            ServiceError::BadGateway(_) => StatusCode::BAD_GATEWAY,
            ServiceError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            ServiceError::NotFound(m)
            | ServiceError::BadRequest(m)
            | ServiceError::Conflict(m)
            | ServiceError::Unauthorized(m)
            | ServiceError::BadGateway(m)
            | ServiceError::Internal(m) => m,
        }
    }
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = Json(json!({ "error": self.message() }));
        (status, body).into_response()
    }
}

pub type ServiceResult<T> = Result<T, ServiceError>;
