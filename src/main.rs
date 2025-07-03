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
#[cfg(feature = "export-postgres")]
use exporter::postgresql::PostgresqlExporter;

#[cfg(feature = "export-parquet")]
use exporter::parquet::ParquetExporter;

use exporter::Exporter;
use libsql::Connection;
use middleware::{validate_body_length, validate_content_type};
use responses::{get_metrics, post_event};
use rust_web_common::telemetry::TelemetryBuilder;
use std::{ops::Deref, sync::Arc};
use storage::memory::initialize;
use tokio::{select, signal::unix::SignalKind, sync::RwLock};
use tokio::{
    spawn,
    time::{Duration, interval},
};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use tracing::{Instrument, error, instrument};
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

    #[cfg(feature = "export-postgres")]
    let periodic_postgres_export_handler =
        spawn(periodic_postgres_export_handler(memory_database.clone()));

    #[cfg(not(feature = "export-postgres"))]
    let periodic_postgres_export_handler = spawn(async {});

    #[cfg(feature = "export-parquet")]
    let periodic_parquet_export_handler =
        spawn(periodic_parquet_export_handler(memory_database.clone()));

    #[cfg(not(feature = "export-parquet"))]
    let periodic_parquet_export_handler = spawn(async {});

    let internal_endpoint_handler = spawn(internal_endpoint_handler(memory_database.clone()));
    let external_endpoint_handler = spawn(external_endpoint_handler(memory_database.clone()));
    let shutdown_handler = spawn(shutdown_handler(memory_database.clone()));

    select! {
        _ = periodic_postgres_export_handler => {}
        _ = periodic_parquet_export_handler => {}
        _ = internal_endpoint_handler => {}
        _ = external_endpoint_handler => {}
        _ = shutdown_handler => {}
    }
}

#[instrument(name = "shutdown-handler")]
async fn shutdown_handler(connection: Arc<libsql::Connection>) {
    let mut signal = tokio::signal::unix::signal(SignalKind::terminate())
        .expect("failed to install SIGTERM handler");

    signal.recv().await;

    #[cfg(feature = "export-postgres")]
    let mut postgres_exporter = PostgresqlExporter::build()
        .await
        .expect("failed to initialize PostgreSQL exporter");

    #[cfg(feature = "export-postgres")]
    postgres_exporter
        .publish(None, connection.clone())
        .instrument(tracing::info_span!("export-postgres"))
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Failed to flush events to PostgreSQL: {}", e);
            0
        });

    #[cfg(feature = "export-parquet")]
    let mut parquet_exporter = ParquetExporter {
        last_export_at: Utc::now()
            .checked_sub_signed(TimeDelta::minutes(1))
            .unwrap(),
    };

    #[cfg(feature = "export-parquet")]
    parquet_exporter
        .publish(None, connection.clone())
        .instrument(tracing::info_span!("export-parquet"))
        .await
        .unwrap_or_else(|e| {
            tracing::error!("Failed to flush events to PostgreSQL: {}", e);
            0
        });
}

async fn external_endpoint_handler(connection: Arc<Connection>) {
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

async fn internal_endpoint_handler(connection: Arc<Connection>) {
    // This server is dedicated to serving Prometheus metrics for observability purposes.
    // It uses a separate port (($PORT || 8000) + 1) to isolate metrics traffic from application traffic.
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

#[cfg(feature = "export-postgres")]
async fn periodic_postgres_export_handler(memory_connection: Arc<libsql::Connection>) {
    let mut postgres_exporter = PostgresqlExporter::build()
        .await
        .expect("failed to initialize PostgreSQL exporter");

    let mut interval = interval(Duration::from_secs(10)); // flush every 10 seconds

    loop {
        interval.tick().await;

        postgres_exporter
            .publish(None, memory_connection.clone())
            .await
            .unwrap_or_else(|e| {
                error!("failed to flush events to PostgreSQL: {e}");
                0
            });
    }
}

#[cfg(feature = "export-parquet")]
async fn periodic_parquet_export_handler(connection: Arc<libsql::Connection>) -> Result<()> {
    let mut interval = interval(Duration::from_secs(30)); // flush every 30 seconds
    let last_export_at = Arc::new(RwLock::new(Utc::now()));
    let export_closure =
        async |connection: Arc<libsql::Connection>, last_export_at: Arc<RwLock<DateTime<Utc>>>| {
            let last_export_at_copy = last_export_at.clone();
            let last_export_at_copy = last_export_at_copy.read().await;
            let last_export_at_copy = last_export_at_copy.deref().to_owned();

            let mut exporter = exporter::parquet::ParquetExporter {
                last_export_at: last_export_at_copy,
            };

            exporter.publish(None, connection.clone()).await
        };

    loop {
        interval.tick().await;

        let exported_started = Utc::now();

        let handle = spawn(export_closure(connection.clone(), last_export_at.clone()));

        match handle.await {
            Err(err) => {
                tracing::error!("error {}", err);
                continue;
            }
            Ok(result) => {
                if let Err(err) = result {
                    tracing::error!("error {}", err);
                    continue;
                }
            }
        }

        let mut guard = last_export_at.write().await;
        *guard = exported_started;
    }
}
