mod errors;
mod exporter;
mod middleware;
mod schemas;
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
use errors::ApplicationError;
use exporter::Exporter;
use libsql::{Connection, params};
use middleware::{validate_body_length, validate_content_type};
use std::sync::Arc;
use storage::memory::initialize;
use tokio::{select, signal::unix::SignalKind};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::{Instrument, info_span};
use utilities::{generate_uuid_v4, initialize_tracing};

#[derive(Clone, Debug)]
pub struct AppState {
    pub connection: Arc<libsql::Connection>,
    pub validator: Arc<jsonschema::Validator>,
}

#[tokio::main]
async fn main() {
    let providers = initialize_tracing().expect("could not initialize logging/tracing");
    let database = initialize().await.expect("failed to initialize database");
    let database = Arc::new(database);

    select! {
        _ = shutdown_handler(providers) => {}
        _ = server_handler(database.clone()) => {}
        _ = metrics_server_handler(database.clone()) => {}
    }
}

async fn handle_event(
    State(state): State<AppState>,
    payload: String,
) -> Result<impl IntoResponse, ApplicationError> {
    let json_payload = serde_json::from_str(&payload)
        .map_err(|e| ApplicationError::InvalidPayload(e.to_string()))?;

    state
        .validator
        .validate(&json_payload)
        .map_err(|e| ApplicationError::InvalidPayload(e.to_string()))?;

    state
        .connection
        .execute(
            "INSERT INTO events (recorded_at, event) VALUES (?1, ?2)",
            params!(Utc::now().to_rfc3339(), payload),
        )
        .instrument(info_span!("insert_event"))
        .await?;

    Ok((StatusCode::ACCEPTED, String::new()))
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

async fn server_handler(connection: Arc<Connection>) {
    let state = AppState {
        connection,
        validator: Arc::new(
            schemas::event_validator().expect("failed to create JSON schema validator"),
        ),
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

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8002").await.unwrap();

    axum::serve(listener, app)
        .await
        .expect("failed to start server")
}

async fn generate_metrics(
    State((connection, app_id)): State<(Arc<Connection>, String)>,
) -> Result<impl IntoResponse, ApplicationError> {
    let exporter = exporter::prometheus::PrometheusExporter {};
    let metrics = exporter.publish(connection, app_id).await?;
    Ok((StatusCode::OK, metrics))
}

async fn metrics_server_handler(connection: Arc<Connection>) {
    // This server is dedicated to serving Prometheus metrics for observability purposes.
    // It uses a separate port (8000) to isolate metrics traffic from application traffic.
    let app_id = generate_uuid_v4();
    let app = Router::new()
        .route("/metrics", get(generate_metrics))
        .with_state((connection, app_id))
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8001").await.unwrap();

    axum::serve(listener, app)
        .await
        .expect("failed to start server")
}
