use anyhow::Result;
use tracing_subscriber::{Layer, layer::SubscriberExt, util::SubscriberInitExt};

pub fn initialize_tracing() -> Result<()> {
    let target = tracing_subscriber::filter::Targets::new()
        .with_default(tracing::level_filters::LevelFilter::DEBUG);

    let fmt = tracing_subscriber::fmt::layer()
        .with_level(true)
        .with_filter(target);

    tracing_subscriber::registry().with(fmt).init();

    Ok(())
}
