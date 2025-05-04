// errors.rs
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use tracing::error;

#[allow(dead_code)]
#[derive(Debug)]
pub struct ServerError(pub anyhow::Error);

impl<E> From<E> for ServerError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        ServerError(err.into())
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        error!("Error: {:?}", self.0);

        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Internal Server Error".to_string(),
        )
            .into_response()
    }
}
