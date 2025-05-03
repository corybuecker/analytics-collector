use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use tracing_subscriber::{Layer, Registry, layer::SubscriberExt, util::SubscriberInitExt};

pub enum Provider {
    #[allow(dead_code)]
    MeterProvider(opentelemetry_sdk::metrics::SdkMeterProvider),
    TracerProvider(opentelemetry_sdk::trace::SdkTracerProvider),
}

pub fn initialize_tracing() -> anyhow::Result<Vec<Provider>> {
    let target = tracing_subscriber::filter::Targets::new()
        .with_default(tracing::level_filters::LevelFilter::DEBUG);

    let fmt: Box<dyn Layer<Registry> + Send + Sync> = tracing_subscriber::fmt::layer()
        .with_level(true)
        .with_filter(target)
        .boxed();

    let mut layers: Vec<Box<dyn Layer<Registry> + Send + Sync>> = vec![fmt];
    let mut tracer_provider: Option<opentelemetry_sdk::trace::SdkTracerProvider> = None;

    if let Ok(tracing_endpoint) = std::env::var("TRACING_ENDPOINT") {
        let exporter = opentelemetry_otlp::SpanExporter::builder()
            .with_http()
            .with_protocol(opentelemetry_otlp::Protocol::HttpBinary)
            .with_endpoint(tracing_endpoint)
            .build()
            .expect("Failed to create span exporter");

        let inner_tracer_provider = opentelemetry_sdk::trace::SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_resource(
                opentelemetry_sdk::Resource::builder()
                    .with_service_name("analytics-collector")
                    .build(),
            )
            .build();

        let tracer = inner_tracer_provider.tracer("analytics-collector");

        let layer: Box<dyn Layer<Registry> + Send + Sync> =
            tracing_opentelemetry::layer().with_tracer(tracer).boxed();

        tracer_provider = Some(inner_tracer_provider);

        layers.push(layer);
    }

    tracing_subscriber::registry().with(layers).init();

    let mut providers = Vec::new();

    if let Some(tracer_provider) = tracer_provider {
        providers.push(Provider::TracerProvider(tracer_provider));
    }

    Ok(providers)
}
