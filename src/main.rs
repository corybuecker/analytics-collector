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
use exporter::{Exporter, postgresql};
use libsql::{Connection, params};
use middleware::{validate_body_length, validate_content_type};
use std::{sync::Arc, time::Duration};
use storage::memory::initialize;
use tokio::{select, signal::unix::SignalKind, sync::RwLock, time::sleep_until};
use tokio_postgres::{Client, NoTls, connect};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::{Instrument, info, info_span};
use utilities::{generate_uuid_v4, initialize_tracing};

#[derive(Clone, Debug)]
pub struct AppState {
    pub connection: Arc<libsql::Connection>,
    pub validator: Arc<jsonschema::Validator>,
}

#[tokio::main]
async fn main() {
    let providers = initialize_tracing().expect("could not initialize logging/tracing");
    let memory_database = initialize().await.expect("failed to initialize database");
    let memory_database = Arc::new(memory_database);
    let postgres_database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL not set");
    let (postgres_client, _) = connect(&postgres_database_url, NoTls)
        .await
        .expect("failed to connect to postgres");
    let postgres_client = Arc::new(RwLock::new(postgres_client));

    select! {
        _ = shutdown_handler(providers) => {}
        _ = server_handler(memory_database.clone()) => {}
        _ = metrics_server_handler(memory_database.clone()) => {}
        _ = database_connection_handler(postgres_client.clone()) => {}
        _ = flush_to_database(memory_database.clone(), postgres_client.clone()) => {}
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
            "INSERT INTO events (id, recorded_at, event) VALUES (?1, ?2, ?3)",
            params!(generate_uuid_v4(), Utc::now().to_rfc3339(), payload),
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

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await.unwrap();

    axum::serve(listener, app)
        .await
        .expect("failed to start server")
}

async fn generate_metrics(
    State((connection, app_id)): State<(Arc<Connection>, String)>,
) -> Result<impl IntoResponse, ApplicationError> {
    let exporter = exporter::prometheus::PrometheusExporter {};
    let mut buffer = String::new();
    exporter.publish(app_id, connection, &mut buffer).await?;
    Ok((StatusCode::OK, ()))
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

async fn database_connection_handler(client: Arc<RwLock<Client>>) {
    // Get the database URL from the environment variable
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL not set");

    loop {
        // Try to connect to the database
        let (replacement_client, connection) = match connect(&database_url, NoTls).await {
            Ok((client, connection)) => {
                info!("Connected to database");
                (client, connection)
            }
            // If connection fails, log the error and retry after 5 seconds
            Err(e) => {
                tracing::error!("Failed to connect to database: {}", e);
                sleep_until(tokio::time::Instant::now() + Duration::from_secs(5)).await;
                continue;
            }
        };

        // Replace the current client with the new one.
        // Acquire a write lock on the Arc-wrapped RwLock<Client> to ensure exclusive access,
        // so that no other task is reading or writing to the client while we update it.
        let mut guard = client.write().await;

        // Overwrite the existing client with the newly established replacement_client.
        // This allows the rest of the application to transparently use the new connection
        // without needing to restart or reinitialize any consumers of the client.
        *guard = replacement_client;

        // Explicitly drop the guard to release the write lock as soon as possible,
        // allowing other tasks to acquire the lock and use the updated client.
        drop(guard);

        // Wait for the connection to finish, log errors if any, and loop to reconnect
        if let Err(e) = connection.await {
            tracing::error!("Connection error: {}", e);
            continue;
        }
    }
}

/// Periodically flushes all events from the in-memory database to the PostgreSQL instance.
///
/// # Arguments
/// * `memory_connection` - Arc pointer to the in-memory libsql::Connection
/// * `postgres_client` - Arc pointer to a RwLock-wrapped tokio_postgres::Client
async fn flush_to_database(
    memory_connection: Arc<libsql::Connection>,
    postgres_client: Arc<RwLock<Client>>,
) {
    use tokio::time::{Duration, interval};

    let mut interval = interval(Duration::from_secs(10)); // flush every 10 seconds
    let exporter = postgresql::PostgresqlExporter {};

    loop {
        interval.tick().await;

        exporter
            .publish(
                "test_app".to_string(),
                memory_connection.clone(),
                postgres_client.clone(),
            )
            .await
            .unwrap_or_else(|e| {
                tracing::error!("Failed to flush events to PostgreSQL: {}", e);
            });
    }
}
