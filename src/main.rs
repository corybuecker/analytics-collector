mod errors;
mod middleware;
mod utilities;

use axum::{
    Router,
    http::StatusCode,
    middleware::from_fn,
    routing::{get, post},
};
use middleware::{validate_body_length, validate_content_type};
use tokio::{select, signal::unix::SignalKind};
use tower::ServiceBuilder;
use tower_http::trace::TraceLayer;
use utilities::initialize_tracing;

#[tokio::main]
async fn main() {
    let providers = initialize_tracing().expect("could not initialize logging/tracing");

    select! {
        _ = shutdown_handler(providers) => {}
        _ = server_handler() => {}
    }
}

async fn handle_event() -> String {
    "test".to_string()
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

async fn server_handler() {
    let app = Router::new()
        .route("/", post(handle_event))
        .route("/{any}", post(handle_event))
        .layer(
            ServiceBuilder::new()
                .layer(from_fn(validate_content_type))
                .layer(from_fn(validate_body_length))
                .layer(TraceLayer::new_for_http()),
        )
        // putting the healthcheck route at the end to avoid it being processed by the middleware and logging
        .route("/healthcheck", get(StatusCode::OK));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8000").await.unwrap();

    axum::serve(listener, app)
        .await
        .expect("failed to start server")
}
