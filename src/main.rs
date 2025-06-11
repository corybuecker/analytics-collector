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
use exporter::{Exporter, postgresql::PostgresqlExporter};
use libsql::{Connection, params};
use middleware::{validate_body_length, validate_content_type};
use rust_web_common::telemetry::TelemetryBuilder;
use std::sync::Arc;
use storage::memory::initialize;
use tokio::time::{Duration, interval};
use tokio::{select, signal::unix::SignalKind};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::{Instrument, info_span};
use utilities::{generate_uuid_v4, get_environment_variable_with_default};

#[derive(Clone, Debug)]
pub struct AppState {
    pub connection: Arc<libsql::Connection>,
    pub validator: Arc<jsonschema::Validator>,
}

#[tokio::main]
async fn main() {
    let _telemetry_providers = TelemetryBuilder::new("analytics-collector".to_string())
        .build()
        .expect("failed to initialize telemetry");
    let memory_database = initialize().await.expect("failed to initialize database");
    let memory_database = Arc::new(memory_database);
    let postgres_exporter = PostgresqlExporter::build()
        .await
        .expect("failed to initialize PostgreSQL exporter");

    select! {
        _ = shutdown_handler(memory_database.clone(), postgres_exporter.clone()) => {}
        _ = server_handler(memory_database.clone()) => {}
        _ = metrics_server_handler(memory_database.clone()) => {}
        _ = flush_to_database(memory_database.clone(), postgres_exporter.clone()) => {}
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

    let recorded_by = json_payload
        .get("appId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ApplicationError::InvalidPayload("Missing 'recorded_by' field".to_string())
        })?;

    state
        .connection
        .execute(
            "INSERT INTO events (id, recorded_at, recorded_by, event) VALUES (?1, ?2, ?3, ?4)",
            params!(
                generate_uuid_v4(),
                Utc::now().to_rfc3339(),
                recorded_by,
                payload
            ),
        )
        .instrument(info_span!("insert_event"))
        .await?;

    Ok((StatusCode::ACCEPTED, String::new()))
}

async fn shutdown_handler(
    memory_connection: Arc<libsql::Connection>,
    mut postgresql_exporter: exporter::postgresql::PostgresqlExporter,
) {
    let mut signal = tokio::signal::unix::signal(SignalKind::terminate())
        .expect("failed to install SIGTERM handler");

    signal.recv().await;

    postgresql_exporter
        .publish(None, memory_connection.clone())
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Failed to flush events to PostgreSQL: {}", e);
            0
        });
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

    let port = get_environment_variable_with_default("PORT", "8000".to_string());
    let port = port.parse::<u16>().unwrap_or(8000);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .unwrap();

    axum::serve(listener, app)
        .await
        .expect("failed to start server")
}

async fn generate_metrics(
    State((connection, instance_id)): State<(Arc<Connection>, String)>,
) -> Result<impl IntoResponse, ApplicationError> {
    let mut exporter = exporter::prometheus::PrometheusExporter {
        buffer: &mut String::new(),
    };
    exporter.publish(Some(instance_id), connection).await?;
    Ok((StatusCode::OK, exporter.buffer.clone()))
}

async fn metrics_server_handler(connection: Arc<Connection>) {
    // This server is dedicated to serving Prometheus metrics for observability purposes.
    // It uses a separate port (8000) to isolate metrics traffic from application traffic.
    let app_id = generate_uuid_v4();
    let app = Router::new()
        .route("/metrics", get(generate_metrics))
        .with_state((connection, app_id))
        .layer(TraceLayer::new_for_http());

    let port = get_environment_variable_with_default("PORT", "8000".to_string());
    let port = port.parse::<u16>().unwrap_or(8000) + 1;
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .unwrap();

    axum::serve(listener, app)
        .await
        .expect("failed to start server")
}

/// Periodically flushes all events from the in-memory database to the PostgreSQL instance.
///
/// # Arguments
/// * `memory_connection` - Arc pointer to the in-memory libsql::Connection
/// * `postgres_client` - Arc pointer to a RwLock-wrapped tokio_postgres::Client
async fn flush_to_database(
    memory_connection: Arc<libsql::Connection>,
    mut postgresql_exporter: exporter::postgresql::PostgresqlExporter,
) {
    let mut interval = interval(Duration::from_secs(10)); // flush every 10 seconds

    loop {
        interval.tick().await;

        postgresql_exporter
            .publish(None, memory_connection.clone())
            .await
            .unwrap_or_else(|e| {
                tracing::error!("Failed to flush events to PostgreSQL: {}", e);
                0
            });
    }
}
