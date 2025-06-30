use crate::{exporter::Exporter, storage::memory::flush};
use arrow::{
    array::{RecordBatch, StringArray},
    datatypes::Field,
};
use arrow_array::ArrayRef;
use arrow_schema::{Schema, SchemaBuilder};
use chrono::{DateTime, Utc};
use parquet::arrow::ArrowWriter;
use std::sync::Arc;
use tracing::{debug, info};

pub struct ParquetExporter<'a> {
    pub buffer: &'a mut Vec<u8>,
    #[allow(dead_code)]
    pub last_export_at: DateTime<Utc>,
}

fn schema() -> Arc<Schema> {
    let mut builder = SchemaBuilder::new();

    let event_fields = vec![
        Field::new("ts", arrow::datatypes::DataType::Utf8, true),
        Field::new("entity", arrow::datatypes::DataType::Utf8, false),
        Field::new("action", arrow::datatypes::DataType::Utf8, false),
        Field::new("path", arrow::datatypes::DataType::Utf8, true),
        Field::new("app_id", arrow::datatypes::DataType::Utf8, false),
    ];

    builder.push(Field::new("id", arrow::datatypes::DataType::Utf8, false));
    builder.push(Field::new_struct("event", event_fields, false));
    builder.push(Field::new(
        "recorded_at",
        arrow::datatypes::DataType::Utf8,
        false,
    ));
    builder.push(Field::new(
        "recorded_by",
        arrow::datatypes::DataType::Utf8,
        true,
    ));

    Arc::new(builder.finish())
}

impl Exporter for ParquetExporter<'_> {
    async fn publish(
        &mut self,
        _exporter_identifier: Option<String>,
        source: Arc<libsql::Connection>,
    ) -> anyhow::Result<usize> {
        info!("Starting parquet export");
        debug!("Querying events from database");
        let mut id_values = Vec::<String>::new();

        let mut event_ts_values = Vec::<Option<String>>::new();
        let mut event_entity_values = Vec::<String>::new();
        let mut event_action_values = Vec::<String>::new();
        let mut event_path_values = Vec::<Option<String>>::new();
        let mut event_app_id_values = Vec::<String>::new();

        let mut recorded_at_values = Vec::<String>::new();
        let mut recorded_by_values = Vec::<Option<String>>::new();

        for event_record in flush(source.clone()).await? {
            id_values.push(event_record.id);

            event_ts_values.push(event_record.event.ts.map(|t| t.to_rfc3339()));
            event_entity_values.push(event_record.event.entity);
            event_action_values.push(event_record.event.action);
            event_path_values.push(event_record.event.path);
            event_app_id_values.push(event_record.event.app_id);

            recorded_at_values.push(event_record.recorded_at.to_rfc3339());
            recorded_by_values.push(event_record.recorded_by);
        }

        let event_values = arrow_array::StructArray::from(vec![
            (
                Arc::new(arrow_schema::Field::new(
                    "ts",
                    arrow_schema::DataType::Utf8,
                    true,
                )),
                (Arc::new(arrow_array::StringArray::from(event_ts_values)) as ArrayRef),
            ),
            (
                Arc::new(Field::new(
                    "entity",
                    arrow::datatypes::DataType::Utf8,
                    false,
                )),
                Arc::new(StringArray::from(event_entity_values)),
            ),
            (
                Arc::new(Field::new(
                    "action",
                    arrow::datatypes::DataType::Utf8,
                    false,
                )),
                Arc::new(StringArray::from(event_action_values)),
            ),
            (
                Arc::new(Field::new("path", arrow::datatypes::DataType::Utf8, true)),
                Arc::new(StringArray::from(event_path_values)),
            ),
            (
                Arc::new(Field::new(
                    "app_id",
                    arrow::datatypes::DataType::Utf8,
                    false,
                )),
                Arc::new(StringArray::from(event_app_id_values)),
            ),
        ]);

        let row_count = id_values.len();

        info!("Processed {} total rows from database", row_count);

        debug!("Creating RecordBatch with {} rows", row_count);
        let parquet_writer = RecordBatch::try_new(
            schema(),
            vec![
                Arc::new(StringArray::from(id_values)),
                Arc::new(event_values),
                Arc::new(StringArray::from(recorded_at_values)),
                Arc::new(StringArray::from(recorded_by_values)),
            ],
        )?;

        debug!("Initializing parquet writer");
        let mut buffer = Vec::<u8>::new();
        let mut writer = ArrowWriter::try_new(&mut buffer, parquet_writer.schema(), None)?;

        debug!("Writing RecordBatch to parquet format");
        writer.write(&parquet_writer)?;
        writer.close()?;

        debug!("Parquet data written, buffer size: {} bytes", buffer.len());
        self.buffer.extend_from_slice(&buffer);

        info!(
            "Parquet export completed successfully, exported {} rows",
            row_count
        );
        Ok(row_count)
    }
}
