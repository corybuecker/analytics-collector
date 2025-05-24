use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use std::fs::File;
use std::io::Read;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{Layer, Registry, layer::SubscriberExt, util::SubscriberInitExt};

pub enum Provider {
    #[allow(dead_code)]
    MeterProvider(opentelemetry_sdk::metrics::SdkMeterProvider),
    TracerProvider(opentelemetry_sdk::trace::SdkTracerProvider),
}

pub fn generate_uuid_v4() -> String {
    let mut bytes = [0u8; 16];
    // Read random bytes from /dev/urandom
    File::open("/dev/urandom")
        .expect("cannot open /dev/urandom")
        .read_exact(&mut bytes)
        .expect("cannot read random bytes");

    // Set version (4) and variant (10)
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    format!(
        "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
        u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        u16::from_be_bytes([bytes[4], bytes[5]]),
        u16::from_be_bytes([bytes[6], bytes[7]]),
        u16::from_be_bytes([bytes[8], bytes[9]]),
        u64::from_be_bytes([
            bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15], 0, 0
        ]) >> 16
    )
}

pub fn initialize_tracing() -> anyhow::Result<Vec<Provider>> {
    let target = tracing_subscriber::filter::Targets::new()
        .with_default(tracing::level_filters::LevelFilter::DEBUG)
        .with_target("tokio_postgres::prepare", LevelFilter::ERROR)
        .with_target("tokio_postgres::query", LevelFilter::ERROR);

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
