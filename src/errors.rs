use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tracing::error;

#[allow(dead_code)]
#[derive(Debug)]
pub enum ApplicationError {
    Unknown(anyhow::Error),
    InvalidPayload(String),
}

impl<E> From<E> for ApplicationError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        ApplicationError::Unknown(err.into())
    }
}

impl IntoResponse for ApplicationError {
    fn into_response(self) -> Response {
        error!("Error: {:?}", self);

        match self {
            ApplicationError::InvalidPayload(e) => (StatusCode::BAD_REQUEST, e).into_response(),
            ApplicationError::Unknown(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal Server Error".to_string(),
            )
                .into_response(),
        }
    }
}
