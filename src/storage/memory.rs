use super::SCHEMA;
use anyhow::Result;
use chrono::{DateTime, Utc};
use libsql::{Builder, Connection, de::from_row, params};
use serde::{Deserialize, Deserializer, de::Visitor};
use std::sync::Arc;
use tokio_stream::StreamExt;
use tracing::error;

#[derive(Debug)]
#[allow(dead_code)]
pub struct Event {
    pub ts: Option<DateTime<Utc>>,
    pub entity: String,
    pub action: String,
    pub path: Option<String>,
    pub app_id: String,
}

struct EventVisitor;

impl<'de> Visitor<'de> for EventVisitor {
    type Value = Event;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "not quite sure yet")
    }

    fn visit_string<E>(self, v: String) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        #[derive(Deserialize)]
        struct IntermediateEvent {
            ts: Option<DateTime<Utc>>,
            entity: String,
            action: String,
            path: Option<String>,

            #[serde(rename = "appId")]
            app_id: String,
        }

        let intermediate: IntermediateEvent =
            serde_json::from_str(&v).map_err(serde::de::Error::custom)?;

        Ok(Event {
            ts: intermediate.ts,
            entity: intermediate.entity,
            action: intermediate.action,
            path: intermediate.path,
            app_id: intermediate.app_id,
        })
    }

    fn visit_str<E>(self, v: &str) -> std::result::Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        #[derive(Deserialize)]
        struct IntermediateEvent {
            ts: Option<DateTime<Utc>>,
            entity: String,
            action: String,
            path: Option<String>,

            #[serde(rename = "appId")]
            app_id: String,
        }

        let intermediate: IntermediateEvent =
            serde_json::from_str(v).map_err(serde::de::Error::custom)?;

        Ok(Event {
            ts: intermediate.ts,
            entity: intermediate.entity,
            action: intermediate.action,
            path: intermediate.path,
            app_id: intermediate.app_id,
        })
    }
}

impl<'de> Deserialize<'de> for Event {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(EventVisitor)
    }
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
pub struct EventRecord {
    pub id: String,
    pub recorded_at: DateTime<Utc>,
    pub recorded_by: Option<String>,
    pub event: Event,
}

pub async fn initialize() -> Result<Connection> {
    let memory_database = Builder::new_local(":memory:")
        .build()
        .await
        .expect("failed to create in-memory database");

    let connection = memory_database.connect()?;
    connection.execute(SCHEMA, ()).await?;

    Ok(connection)
}

pub async fn flush_since(
    connection: Arc<Connection>,
    since: DateTime<Utc>,
) -> Result<Vec<EventRecord>> {
    let rows = connection
        .query(
            "SELECT id, event, recorded_by, recorded_at FROM events WHERE recorded_at > ?",
            params!(since.to_rfc3339()),
        )
        .await?;

    Ok(rows
        .into_stream()
        .filter_map(|row| match row {
            Ok(valid_row) => from_row::<EventRecord>(&valid_row).ok(),
            Err(e) => {
                error!("Failed to process row: {:?}", e);
                None
            }
        })
        .collect::<Vec<EventRecord>>()
        .await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::json;

    #[test]
    fn test_event_record_deserialization_success() {
        let json_data = json!({
            "id": "test-id-123",
            "recorded_at": "2023-01-01T12:00:00Z",
            "recorded_by": "test-user",
            "event": r#"{
                "ts": "2023-01-01T10:00:00Z",
                "entity": "user",
                "action": "login",
                "path": "/login",
                "appId": "my-app"
            }"#
        });

        let event_record: EventRecord = serde_json::from_value(json_data).unwrap();

        assert_eq!(event_record.id, "test-id-123");
        assert_eq!(event_record.recorded_by, Some("test-user".to_string()));
        assert_eq!(event_record.event.entity, "user");
        assert_eq!(event_record.event.action, "login");
        assert_eq!(event_record.event.path, Some("/login".to_string()));
        assert_eq!(event_record.event.app_id, "my-app");
        assert_eq!(
            event_record.event.ts,
            Some(Utc.with_ymd_and_hms(2023, 1, 1, 10, 0, 0).unwrap())
        );
    }

    #[test]
    fn test_event_record_deserialization_with_null_fields() {
        let json_data = json!({
            "id": "test-id-456",
            "recorded_at": "2023-01-01T12:00:00Z",
            "recorded_by": null,
            "event": r#"{
                "ts": null,
                "entity": "page",
                "action": "view",
                "path": null,
                "appId": "my-app"
            }"#
        });

        let event_record: EventRecord = serde_json::from_value(json_data).unwrap();

        assert_eq!(event_record.id, "test-id-456");
        assert_eq!(event_record.recorded_by, None);
        assert_eq!(event_record.event.entity, "page");
        assert_eq!(event_record.event.action, "view");
        assert_eq!(event_record.event.path, None);
        assert_eq!(event_record.event.ts, None);
        assert_eq!(event_record.event.app_id, "my-app");
    }

    #[test]
    fn test_event_record_deserialization_invalid_event_json() {
        let json_data = json!({
            "id": "test-id-789",
            "recorded_at": "2023-01-01T12:00:00Z",
            "recorded_by": "test-user",
            "event": "invalid json string"
        });

        let result: Result<EventRecord, _> = serde_json::from_value(json_data);
        assert!(result.is_err());
    }

    #[test]
    fn test_event_record_deserialization_missing_required_fields() {
        let json_data = json!({
            "id": "test-id-999",
            "recorded_at": "2023-01-01T12:00:00Z",
            "recorded_by": "test-user",
            "event": r#"{
                "entity": "user",
                "action": "login"
            }"#
        });

        let result: Result<EventRecord, _> = serde_json::from_value(json_data);
        assert!(result.is_err());
    }

    #[test]
    fn test_event_deserialization_from_string_wrapper() {
        // Test that Event can be deserialized when it's a JSON string value
        let wrapper_json = json!({
            "event_data": r#"{
                "ts": "2023-06-15T14:30:00Z",
                "entity": "product",
                "action": "purchase",
                "path": "/checkout",
                "appId": "ecommerce-app"
            }"#
        });

        #[derive(Deserialize)]
        struct EventWrapper {
            event_data: Event,
        }

        let wrapper: EventWrapper = serde_json::from_value(wrapper_json).unwrap();

        assert_eq!(wrapper.event_data.entity, "product");
        assert_eq!(wrapper.event_data.action, "purchase");
        assert_eq!(wrapper.event_data.path, Some("/checkout".to_string()));
        assert_eq!(wrapper.event_data.app_id, "ecommerce-app");
        assert_eq!(
            wrapper.event_data.ts,
            Some(Utc.with_ymd_and_hms(2023, 6, 15, 14, 30, 0).unwrap())
        );
    }

    #[test]
    fn test_event_deserialization_camel_case_mapping() {
        // Test that appId is correctly mapped to app_id
        let wrapper_json = json!({
            "event_data": r#"{
                "ts": "2023-12-25T00:00:00Z",
                "entity": "gift",
                "action": "unwrap",
                "path": "/presents",
                "appId": "holiday-tracker"
            }"#
        });

        #[derive(Deserialize)]
        struct EventWrapper {
            event_data: Event,
        }

        let wrapper: EventWrapper = serde_json::from_value(wrapper_json).unwrap();
        assert_eq!(wrapper.event_data.app_id, "holiday-tracker");
    }

    #[test]
    fn test_complete_event_record_roundtrip() {
        let original_json = json!({
            "id": "roundtrip-test",
            "recorded_at": "2023-07-04T16:45:30Z",
            "recorded_by": "system",
            "event": r#"{
                "ts": "2023-07-04T16:45:00Z",
                "entity": "celebration",
                "action": "fireworks",
                "path": "/independence-day",
                "appId": "patriot-app"
            }"#
        });

        // Deserialize
        let event_record: EventRecord = serde_json::from_value(original_json.clone()).unwrap();

        // Verify all fields are correctly deserialized
        assert_eq!(event_record.id, "roundtrip-test");
        assert_eq!(event_record.recorded_by, Some("system".to_string()));
        assert_eq!(event_record.event.entity, "celebration");
        assert_eq!(event_record.event.action, "fireworks");
        assert_eq!(
            event_record.event.path,
            Some("/independence-day".to_string())
        );
        assert_eq!(event_record.event.app_id, "patriot-app");

        let expected_recorded_at = Utc.with_ymd_and_hms(2023, 7, 4, 16, 45, 30).unwrap();
        let expected_event_ts = Utc.with_ymd_and_hms(2023, 7, 4, 16, 45, 0).unwrap();

        assert_eq!(event_record.recorded_at, expected_recorded_at);
        assert_eq!(event_record.event.ts, Some(expected_event_ts));
    }
}
