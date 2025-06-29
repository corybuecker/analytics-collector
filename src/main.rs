mod errors;
mod exporter;
mod middleware;
mod responses;
mod schemas;
mod storage;
mod utilities;

use anyhow::Result;
use axum::{
    Router,
    http::StatusCode,
    middleware::from_fn,
    routing::{get, post},
};
use chrono::{DateTime, TimeDelta, Utc};
use exporter::{Exporter, postgresql::PostgresqlExporter};
use libsql::Connection;
use middleware::{validate_body_length, validate_content_type};
use responses::{get_metrics, post_event};
use rust_web_common::telemetry::TelemetryBuilder;
use std::{
    ops::Deref,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use storage::{
    google_storage::GoogleStorageClient,
    memory::{flush, initialize},
};
use tokio::{select, signal::unix::SignalKind, sync::RwLock};
use tokio::{
    spawn,
    time::{Duration, interval},
};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::debug;
use utilities::{generate_uuid_v4, get_environment_variable_with_default};

/// Application state shared across HTTP request handlers.
///
/// This struct contains the core dependencies needed by the analytics collector service,
/// including database connectivity and JSON schema validation capabilities. It is designed
/// to be efficiently cloned across async tasks using `Arc` for shared ownership.
///
/// # Fields
///
/// * `connection` - Thread-safe reference to the LibSQL database connection used for
///   storing analytics events in the in-memory database
/// * `validator` - Thread-safe reference to the JSON schema validator used to validate
///   incoming event payloads against the expected schema
///
/// # Usage
///
/// The `AppState` is typically passed to Axum route handlers via the `State` extractor:
///
/// ```rust,ignore
/// async fn handle_event(
///     State(state): State<AppState>,
///     payload: String,
/// ) -> Result<impl IntoResponse, ApplicationError> {
///     // Use state.connection for database operations
///     // Use state.validator for payload validation
/// }
/// ```
#[derive(Clone, Debug)]
pub struct AppState {
    /// Thread-safe reference to the LibSQL database connection.
    ///
    /// This connection is used to store validated analytics events in the in-memory
    /// database before they are exported to PostgreSQL.
    pub connection: Arc<libsql::Connection>,

    /// Thread-safe reference to the JSON schema validator.
    ///
    /// This validator ensures that incoming event payloads conform to the expected
    /// JSON schema before they are processed and stored.
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
        _ = public_endpoint_handler(memory_database.clone()) => {}
        _ = private_endpoint_handler(memory_database.clone()) => {}
        _ = periodic_parquet_export_handler(memory_database.clone()) => {}
        _ = periodic_export_handler(memory_database.clone(), postgres_exporter.clone()) => {}
    }
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

    export_rows_as_parquet(
        memory_connection.clone(),
        Arc::new(RwLock::new(
            Utc::now()
                .checked_sub_signed(TimeDelta::minutes(1))
                .unwrap(),
        )),
    )
    .await
    .expect("did not export parquet");
}

async fn public_endpoint_handler(connection: Arc<Connection>) {
    let state = AppState {
        connection,
        validator: Arc::new(
            schemas::event_validator().expect("failed to create JSON schema validator"),
        ),
    };
    let app = Router::new()
        .route("/", post(post_event))
        .route("/{any}", post(post_event))
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

async fn private_endpoint_handler(connection: Arc<Connection>) {
    // This server is dedicated to serving Prometheus metrics for observability purposes.
    // It uses a separate port (8000) to isolate metrics traffic from application traffic.
    let app_id = generate_uuid_v4();
    let app = Router::new()
        .route("/metrics", get(get_metrics))
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
async fn periodic_export_handler(
    memory_connection: Arc<libsql::Connection>,
    mut postgresql_exporter: exporter::postgresql::PostgresqlExporter,
) {
    let mut interval = interval(Duration::from_secs(10)); // flush every 10 seconds

    loop {
        interval.tick().await;

        let a = flush(memory_connection.clone()).await;

        debug!("{:?}", a);

        postgresql_exporter
            .publish(None, memory_connection.clone())
            .await
            .unwrap_or_else(|e| {
                tracing::error!("Failed to flush events to PostgreSQL: {}", e);
                0
            });
    }
}

async fn export_rows_as_parquet(
    connection: Arc<libsql::Connection>,
    last_export_at: Arc<RwLock<DateTime<Utc>>>,
) -> Result<()> {
    let mut buffer = Vec::<u8>::new();

    let last_export_at_copy = last_export_at.clone();
    let last_export_at_copy = last_export_at_copy.read().await;
    let last_export_at_copy = last_export_at_copy.deref().to_owned();

    let mut exporter = exporter::parquet::ParquetExporter {
        buffer: &mut buffer,
        last_export_at: last_export_at_copy,
    };

    let rows = exporter.publish(None, connection.clone()).await?;

    if rows > 0 {
        let mut client = GoogleStorageClient::new()?;
        let now = SystemTime::now();
        let duration = now.duration_since(UNIX_EPOCH)?;
        let micros = duration.as_micros();

        client
            .upload_binary_data(
                &micros.to_string(),
                buffer.as_slice(),
                Some("application/vnd.apache.parquet"),
            )
            .await?;
    }

    Ok(())
}

async fn periodic_parquet_export_handler(connection: Arc<libsql::Connection>) -> Result<()> {
    let mut interval = interval(Duration::from_secs(30)); // flush every 30 seconds
    let last_export_at = Arc::new(RwLock::new(Utc::now()));

    loop {
        interval.tick().await;

        let exported_started = Utc::now();

        let handle = spawn(export_rows_as_parquet(
            connection.clone(),
            last_export_at.clone(),
        ));

        match handle.await {
            Err(err) => tracing::error!("error {}", err),
            Ok(result) => {
                if let Err(err) = result {
                    tracing::error!("error {}", err)
                }
            }
        }

        let mut guard = last_export_at.write().await;
        *guard = exported_started;
    }
}
