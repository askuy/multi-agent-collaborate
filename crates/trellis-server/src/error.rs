use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use trellis_coord::CoordError;
use trellis_core::CoreError;
use trellis_derive::DeriveError;
use trellis_land::LandError;

/// An error response: `{"error": <stable code>, "message": <text>}`.
#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "invalid-input", message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "internal", message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(serde_json::json!({ "error": self.code, "message": self.message })),
        )
            .into_response()
    }
}

impl From<CoreError> for ApiError {
    fn from(e: CoreError) -> Self {
        Self::bad_request(e.to_string())
    }
}

impl From<CoordError> for ApiError {
    fn from(e: CoordError) -> Self {
        let message = e.to_string();
        let (status, code) = match e {
            CoordError::InvalidScope(_) => (StatusCode::BAD_REQUEST, "invalid-scope"),
            CoordError::TaskNotFound(_)
            | CoordError::LeaseNotFound(_)
            | CoordError::EntryNotFound(_) => (StatusCode::NOT_FOUND, "not-found"),
            CoordError::NotTaskOwner { .. } | CoordError::NotLeaseHolder { .. } => {
                (StatusCode::FORBIDDEN, "not-owner")
            }
            CoordError::Store(_) | CoordError::Encoding(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        };
        Self::new(status, code, message)
    }
}

impl From<LandError> for ApiError {
    fn from(e: LandError) -> Self {
        let message = e.to_string();
        let (status, code) = match e {
            LandError::Coord(inner) => return inner.into(),
            LandError::Core(_) => (StatusCode::BAD_REQUEST, "invalid-input"),
            LandError::Policy { .. } => (StatusCode::BAD_REQUEST, "invalid-policy"),
            LandError::NotInitialized => (StatusCode::NOT_FOUND, "repo-not-initialized"),
            LandError::SessionNotFound(_) | LandError::ConflictNotFound(_) => {
                (StatusCode::NOT_FOUND, "not-found")
            }
            LandError::Derive(DeriveError::MissingObject(_) | DeriveError::MissingChangeset(_)) => {
                (StatusCode::NOT_FOUND, "not-found")
            }
            LandError::SessionNotOpen(_) => (StatusCode::CONFLICT, "session-not-open"),
            LandError::ConflictNotCurrent(_) => (StatusCode::CONFLICT, "conflict-not-current"),
            LandError::Unresolved { .. } => (StatusCode::CONFLICT, "unresolved"),
            LandError::BaseNotOnMain => (StatusCode::CONFLICT, "base-not-on-main"),
            LandError::Contention(_) => (StatusCode::SERVICE_UNAVAILABLE, "contention"),
            LandError::Store(_) | LandError::Derive(_) | LandError::Encoding(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal")
            }
        };
        Self::new(status, code, message)
    }
}
