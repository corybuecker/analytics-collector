use anyhow::Result;
use libsql::{Connection, params};
use prometheus_client::{
    encoding::{EncodeLabelSet, text::encode},
    metrics::{counter::Counter, family::Family},
    registry::Registry,
};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Debug, Deserialize, EncodeLabelSet, Clone, Hash, Eq, PartialEq)]
struct Event {
    event_name: String,
    action: Option<String>,
    app_id: Option<String>,
    path: Option<String>,
}

pub async fn publish(connection: Arc<Connection>, app_id: String) -> Result<String> {
    let mut registry = Registry::default();
    let counter = Family::<Event, Counter>::default();

    registry.register("events", "analytics", counter.clone());

    let mut results = connection
        .clone()
        .query("select event from events", params![])
        .await?;

    while let Some(row) = results.next().await? {
        let event: String = row.get(0)?;
        if let Ok(mut event) = serde_json::from_str::<Event>(&event) {
            event.app_id = Some(app_id.clone());
            counter.get_or_create(&event).inc();
        }
    }

    let mut buffer = String::new();
    encode(&mut buffer, &registry)?;
    Ok(buffer)
}
