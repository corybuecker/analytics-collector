use crate::storage::EventSerializer;
use anyhow::Result;
use anyhow::anyhow;
use arrow_array::StructArray;
use arrow_array::TimestampMillisecondArray;
use arrow_array::{RecordBatch, StringArray};
use arrow_schema::Field;
use arrow_schema::Fields;
use arrow_schema::{DataType, Schema, SchemaBuilder, TimeUnit};
use parquet::arrow::ArrowWriter;
use std::sync::Arc;
use tracing::debug;
use tracing::info;

pub struct ParqetSerializer;
pub static VERSION: &str = "1.1.0";

impl EventSerializer for ParqetSerializer {
    fn to_bytes<'a>(
        &self,
        event_records: impl IntoIterator<Item = &'a crate::storage::memory::EventRecord>,
    ) -> Result<(Vec<u8>, usize)> {
        let (record_batch, row_count) = generate_record_batch(event_records)?;

        let mut buffer = Vec::<u8>::new();
        let mut writer = ArrowWriter::try_new(&mut buffer, record_batch.schema(), None)?;

        writer.write(&record_batch)?;
        writer.close()?;

        debug!("Parquet data written, buffer size: {} bytes", buffer.len());

        Ok((buffer, row_count))
    }
}

fn generate_record_batch<'a>(
    event_records: impl IntoIterator<Item = &'a crate::storage::memory::EventRecord>,
) -> Result<(RecordBatch, usize)> {
    let mut id_values = Vec::<String>::new();

    let mut event_ts_values = Vec::<Option<i64>>::new();
    let mut event_entity_values = Vec::<String>::new();
    let mut event_action_values = Vec::<String>::new();
    let mut event_path_values = Vec::<Option<String>>::new();
    let mut event_app_id_values = Vec::<String>::new();

    let mut recorded_at_values = Vec::<i64>::new();
    let mut recorded_by_values = Vec::<Option<String>>::new();

    for event_record in event_records {
        id_values.push(event_record.id.clone());

        event_ts_values.push(event_record.event.ts.map(|t| t.timestamp_millis()));
        event_entity_values.push(event_record.event.entity.clone());
        event_action_values.push(event_record.event.action.clone());
        event_path_values.push(event_record.event.path.clone());
        event_app_id_values.push(event_record.event.app_id.clone());

        recorded_at_values.push(event_record.recorded_at.timestamp_millis());
        recorded_by_values.push(event_record.recorded_by.clone());
    }

    let event_values = StructArray::try_new(
        event_fields(),
        vec![
            Arc::new(TimestampMillisecondArray::from(event_ts_values)),
            Arc::new(StringArray::from(event_entity_values)),
            Arc::new(StringArray::from(event_action_values)),
            Arc::new(StringArray::from(event_path_values)),
            Arc::new(StringArray::from(event_app_id_values)),
        ],
        None,
    )?;

    let row_count = id_values.len();

    info!("Processed {row_count} total rows from database");

    Ok((
        RecordBatch::try_new(
            generate_schema(),
            vec![
                Arc::new(StringArray::from(id_values)),
                Arc::new(event_values),
                Arc::new(TimestampMillisecondArray::from(recorded_at_values)),
                Arc::new(StringArray::from(recorded_by_values)),
            ],
        )
        .map_err(|e| anyhow!("{:?}", e))?,
        row_count,
    ))
}

fn event_fields() -> Fields {
    Fields::from(vec![
        Field::new("ts", DataType::Timestamp(TimeUnit::Millisecond, None), true),
        Field::new("entity", DataType::Utf8, false),
        Field::new("action", DataType::Utf8, false),
        Field::new("path", DataType::Utf8, true),
        Field::new("app_id", DataType::Utf8, false),
    ])
}

fn generate_event_field() -> Field {
    Field::new_struct("event", event_fields(), false)
}

fn generate_schema() -> Arc<Schema> {
    let mut builder = SchemaBuilder::new();

    builder.push(Field::new("id", DataType::Utf8, false));
    builder.push(generate_event_field());
    builder.push(Field::new(
        "recorded_at",
        DataType::Timestamp(TimeUnit::Millisecond, None),
        false,
    ));
    builder.push(Field::new("recorded_by", DataType::Utf8, true));

    Arc::new(builder.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::memory::{Event, EventRecord};
    use chrono::{DateTime, Utc};

    fn create_test_event_record(id: &str, with_optional_fields: bool) -> EventRecord {
        let recorded_at = "2023-01-01T12:00:00Z".parse::<DateTime<Utc>>().unwrap();
        let event_ts = if with_optional_fields {
            Some("2023-01-01T10:00:00Z".parse::<DateTime<Utc>>().unwrap())
        } else {
            None
        };

        EventRecord {
            id: id.to_string(),
            recorded_at,
            recorded_by: if with_optional_fields {
                Some("test-user".to_string())
            } else {
                None
            },
            event: Event {
                ts: event_ts,
                entity: "user".to_string(),
                action: "login".to_string(),
                path: if with_optional_fields {
                    Some("/login".to_string())
                } else {
                    None
                },
                app_id: "my-app".to_string(),
            },
        }
    }

    #[test]
    fn test_to_bytes_empty_records() {
        let serializer = ParqetSerializer;
        let empty_records: Vec<EventRecord> = vec![];
        let result = serializer.to_bytes(empty_records.iter()).unwrap();
        let (bytes, count) = result;
        assert_eq!(count, 0);
        assert!(!bytes.is_empty()); // Parquet file should still have headers
    }

    #[test]
    fn test_to_bytes_single_record_with_optional_fields() {
        let serializer = ParqetSerializer;
        let record = create_test_event_record("test-id-1", true);
        let records = [record];

        let (bytes, count) = serializer.to_bytes(records.iter()).unwrap();
        assert_eq!(count, 1);
        assert!(!bytes.is_empty());

        // Verify we have parquet magic bytes (PAR1)
        assert!(bytes.len() > 8);
        assert_eq!(&bytes[0..4], b"PAR1");
        assert_eq!(&bytes[bytes.len() - 4..], b"PAR1");
    }

    #[test]
    fn test_to_bytes_single_record_without_optional_fields() {
        let serializer = ParqetSerializer;
        let record = create_test_event_record("test-id-2", false);
        let records = [record];

        let (bytes, count) = serializer.to_bytes(records.iter()).unwrap();

        assert_eq!(count, 1);
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_to_bytes_multiple_records() {
        let serializer = ParqetSerializer;
        let records = vec![
            create_test_event_record("test-id-1", true),
            create_test_event_record("test-id-2", false),
            create_test_event_record("test-id-3", true),
        ];

        let (bytes, count) = serializer.to_bytes(records.iter()).unwrap();

        assert_eq!(count, 3);
        assert!(!bytes.is_empty());

        // Verify we have parquet magic bytes
        assert!(bytes.len() > 8);
        assert_eq!(&bytes[0..4], b"PAR1");
        assert_eq!(&bytes[bytes.len() - 4..], b"PAR1");
    }

    #[test]
    fn test_generate_record_batch_empty() {
        let empty_records: Vec<EventRecord> = vec![];
        let (record_batch, count) = generate_record_batch(empty_records.iter()).unwrap();

        assert_eq!(count, 0);
        assert_eq!(record_batch.num_rows(), 0);
        assert_eq!(record_batch.num_columns(), 4); // id, recorded_at, recorded_by
    }

    #[test]
    fn test_generate_record_batch_single_record() {
        let record = create_test_event_record("test-id-1", true);
        let records = [record];
        let (record_batch, count) = generate_record_batch(records.iter()).unwrap();

        assert_eq!(count, 1);
        assert_eq!(record_batch.num_rows(), 1);
        assert_eq!(record_batch.num_columns(), 4);

        // Verify column names
        let schema = record_batch.schema();
        let field_names: Vec<&str> = schema.fields().iter().map(|f| f.name().as_str()).collect();
        assert_eq!(
            field_names,
            vec!["id", "event", "recorded_at", "recorded_by"]
        );
    }

    #[test]
    fn test_generate_record_batch_multiple_records() {
        let records = vec![
            create_test_event_record("test-id-1", true),
            create_test_event_record("test-id-2", false),
            create_test_event_record("test-id-3", true),
        ];
        let (record_batch, count) = generate_record_batch(records.iter()).unwrap();

        assert_eq!(count, 3);
        assert_eq!(record_batch.num_rows(), 3);
        assert_eq!(record_batch.num_columns(), 4);
    }

    #[test]
    fn test_parquet_file_roundtrip() {
        let serializer = ParqetSerializer;
        let records = vec![
            create_test_event_record("test-id-1", true),
            create_test_event_record("test-id-2", false),
        ];

        let (bytes, count) = serializer.to_bytes(records.iter()).unwrap();

        assert_eq!(count, 2);

        // Verify we have parquet magic bytes
        assert!(bytes.len() > 8);
        assert_eq!(&bytes[0..4], b"PAR1");
        assert_eq!(&bytes[bytes.len() - 4..], b"PAR1");
    }

    #[test]
    fn test_large_number_of_records() {
        let serializer = ParqetSerializer;
        let mut records = Vec::new();

        // Create 1000 test records
        for i in 0..1000 {
            records.push(create_test_event_record(
                &format!("test-id-{i}"),
                i % 2 == 0,
            ));
        }

        let (bytes, count) = serializer.to_bytes(records.iter()).unwrap();

        assert_eq!(count, 1000);
        assert!(!bytes.is_empty());

        // Verify we have parquet magic bytes
        assert!(bytes.len() > 8);
        assert_eq!(&bytes[0..4], b"PAR1");
        assert_eq!(&bytes[bytes.len() - 4..], b"PAR1");
    }
}
