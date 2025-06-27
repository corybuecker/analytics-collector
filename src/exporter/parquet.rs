use crate::exporter::Exporter;
use arrow::{
    array::{RecordBatch, StringArray},
    datatypes::Field,
};
use arrow_schema::{Schema, SchemaBuilder};
use libsql::params;
use parquet::arrow::ArrowWriter;
use std::sync::Arc;
use tracing::{debug, info};

pub struct ParquetExporter<'a> {
    pub buffer: &'a mut Vec<u8>,
}

fn schema() -> Arc<Schema> {
    let mut builder = SchemaBuilder::new();
    builder.push(Field::new("id", arrow::datatypes::DataType::Utf8, false));
    builder.push(Field::new("event", arrow::datatypes::DataType::Utf8, false));
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
        let mut event_values = Vec::<String>::new();
        let mut recorded_at_values = Vec::<String>::new();
        let mut recorded_by_values = Vec::<String>::new();

        let mut results = source
            .query(
                "select id, event, recorded_at, recorded_by from events",
                params![],
            )
            .await?;

        debug!("Query executed successfully, processing rows");

        let mut row_count = 0;
        loop {
            let row = results.next().await?;

            match row {
                Some(row) => {
                    let id = row.get_str(0)?;
                    let event = row.get_str(1)?;
                    let recorded_at = row.get_str(2)?;
                    let recorded_by = row.get_str(3)?;

                    id_values.push(id.to_string());
                    event_values.push(event.to_string());
                    recorded_at_values.push(recorded_at.to_string());
                    recorded_by_values.push(recorded_by.to_string());

                    row_count += 1;
                    if row_count % 1000 == 0 {
                        debug!("Processed {} rows", row_count);
                    }
                }
                None => {
                    break;
                }
            }
        }

        info!("Processed {} total rows from database", row_count);

        debug!("Creating RecordBatch with {} rows", row_count);
        let parquet_writer = RecordBatch::try_new(
            schema(),
            vec![
                Arc::new(StringArray::from(id_values)),
                Arc::new(StringArray::from(event_values)),
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
