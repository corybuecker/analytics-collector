use anyhow::Result;
use libsql::params;
use prometheus_client::{
    encoding::{EncodeLabelSet, text::encode},
    metrics::{counter::Counter, family::Family},
    registry::Registry,
};
use serde::Deserialize;
use std::sync::Arc;

use super::Exporter;

#[derive(Debug, Deserialize, EncodeLabelSet, Clone, Hash, Eq, PartialEq)]
struct Event {
    entity: String,
    action: String,
    app_id: Option<String>,
    path: Option<String>,
}

pub struct PrometheusExporter;

impl Exporter for PrometheusExporter {
    async fn publish(&self, connection: Arc<libsql::Connection>, app_id: String) -> Result<String> {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::initialize;
    use chrono::Utc;
    use libsql::{Connection, params};
    use std::sync::Arc;

    async fn setup_db_with_events(events: Vec<&str>) -> Arc<Connection> {
        let connection = initialize().await.unwrap();

        for event in events {
            connection
                .execute(
                    "INSERT INTO events (event, recorded_at) VALUES (?1, ?2)",
                    params![event, Utc::now().to_rfc3339()],
                )
                .await
                .unwrap();
        }
        Arc::new(connection)
    }

    #[tokio::test]
    async fn test_publish_counts_events() {
        let events = vec![
            r#"{"entity":"signup","action":"page_view","path":"/"}"#,
            r#"{"entity":"signup","action":"page_view","path":"/"}"#,
            r#"{"entity":"login","action":"click","path":"/login"}"#,
        ];
        let conn = setup_db_with_events(events).await;
        let app_id = "test-app".to_string();
        let exporter = PrometheusExporter {};
        let metrics = exporter.publish(conn, app_id.clone()).await.unwrap();

        // Should contain entity, action, path, and app_id as labels
        assert!(metrics.contains("entity=\"signup\""));
        assert!(metrics.contains("entity=\"login\""));
        assert!(metrics.contains("action=\"page_view\""));
        assert!(metrics.contains("action=\"click\""));
        assert!(metrics.contains("app_id=\"test-app\""));
        assert!(metrics.contains("path=\"/\""));
        assert!(metrics.contains("path=\"/login\""));

        // Should count two signups and one login
        let signup_count = metrics
            .lines()
            .find(|l| l.contains("entity=\"signup\""))
            .unwrap();
        let signup_count_value: i32 = signup_count
            .rsplit_once(' ')
            .and_then(|(_, count)| count.parse().ok())
            .expect("Failed to parse signup count");
        assert_eq!(signup_count_value, 2);

        let login_count = metrics
            .lines()
            .find(|l| l.contains("entity=\"login\""))
            .unwrap();
        let login_count_value: i32 = login_count
            .rsplit_once(' ')
            .and_then(|(_, count)| count.parse().ok())
            .expect("Failed to parse login count");
        assert_eq!(login_count_value, 1);
    }

    #[tokio::test]
    async fn test_publish_handles_empty_table() {
        let conn = setup_db_with_events(vec![]).await;
        let app_id = "empty-app".to_string();

        let exporter = PrometheusExporter {};
        let metrics = exporter.publish(conn, app_id).await.unwrap();
        // Should still output valid Prometheus format, but no event lines
        assert!(metrics.contains("# TYPE events counter"));
        assert!(!metrics.contains("entity="));
    }

    #[tokio::test]
    async fn test_publish_ignores_invalid_json() {
        let events = vec![
            r#"{"entity":"signup", "action": "click"}"#,
            r#"not a json"#,
            r#"{"entity":"signup", "action": "click"}"#,
        ];
        let conn = setup_db_with_events(events).await;
        let app_id = "bad-json".to_string();

        let exporter = PrometheusExporter {};
        let metrics = exporter.publish(conn, app_id).await.unwrap();
        // Only two valid events should be counted
        let signup_count = metrics
            .lines()
            .find(|l| l.contains("entity=\"signup\""))
            .unwrap();
        let count: u64 = signup_count
            .split_whitespace()
            .last()
            .and_then(|v| v.parse().ok())
            .expect("Failed to parse count from metrics");
        assert_eq!(count, 2);
    }
}
