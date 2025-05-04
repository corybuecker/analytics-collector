mod errors;
mod middleware;
mod storage;
mod utilities;

use anyhow::Result;
use axum::{
    Router,
    extract::State,
    http::StatusCode,
    middleware::from_fn,
    response::IntoResponse,
    routing::{get, post},
};
use chrono::Utc;
use errors::ServerError;
use libsql::{Connection, params};
use middleware::{validate_body_length, validate_content_type};
use std::sync::Arc;
use storage::memory::initialize;
use tokio::{select, signal::unix::SignalKind};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::{Instrument, info_span};
use utilities::initialize_tracing;

#[derive(Clone, Debug)]
pub struct AppState {
    pub connection: Arc<libsql::Connection>,
}

#[tokio::main]
async fn main() {
    let providers = initialize_tracing().expect("could not initialize logging/tracing");
    let database = initialize().await.expect("failed to initialize database");

    select! {
        _ = shutdown_handler(providers) => {}
        _ = server_handler(database) => {}
    }
}

async fn handle_event(
    State(state): State<AppState>,
    payload: String,
) -> Result<impl IntoResponse, ServerError> {
    state
        .connection
        .execute(
            "INSERT INTO events (ts, event) VALUES (?1, ?2)",
            params!(Utc::now().to_rfc3339(), payload),
        )
        .instrument(info_span!("insert_event"))
        .await?;

    Ok(StatusCode::ACCEPTED)
}

async fn shutdown_handler(providers: Vec<utilities::Provider>) {
    let mut signal = tokio::signal::unix::signal(SignalKind::terminate())
        .expect("failed to install SIGTERM handler");

    signal.recv().await;

    for provider in providers {
        match provider {
            utilities::Provider::MeterProvider(provider) => {
                provider
                    .shutdown()
                    .expect("failed to shutdown meter provider");
            }
            utilities::Provider::TracerProvider(tracer_provider) => {
                tracer_provider
                    .shutdown()
                    .expect("failed to shutdown tracer provider");
            }
        }
    }
}

async fn server_handler(connection: Connection) {
    let state = AppState {
        connection: Arc::new(connection),
    };
    let app = Router::new()
        .route("/", post(handle_event))
        .route("/{any}", post(handle_event))
        .layer(
            ServiceBuilder::new()
                .layer(from_fn(validate_content_type))
                .layer(from_fn(validate_body_length))
                .layer(TraceLayer::new_for_http()),
        )
        .with_state(state)
        // putting the healthcheck route at the end to avoid it being processed by the middleware and logging
        .route("/healthcheck", get(StatusCode::OK));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await.unwrap();

    axum::serve(listener, app)
        .await
        .expect("failed to start server")
}
