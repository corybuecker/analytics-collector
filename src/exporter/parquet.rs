use crate::exporter::Exporter;
use arrow::array::{Array, RecordBatch, StringArray};
use libsql::params;
use parquet::arrow::ArrowWriter;
use std::sync::Arc;

pub struct ParquetExporter<'a> {
    pub buffer: &'a mut Vec<u8>,
}

impl Exporter for ParquetExporter<'_> {
    async fn publish(
        &mut self,
        _exporter_identifier: Option<String>,
        source: Arc<libsql::Connection>,
    ) -> anyhow::Result<usize> {
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
                }
                None => {
                    break;
                }
            }
        }

        let id_field: Arc<dyn Array> = Arc::new(StringArray::from(id_values));
        let event_field: Arc<dyn Array> = Arc::new(StringArray::from(event_values));
        let recorded_at_field: Arc<dyn Array> = Arc::new(StringArray::from(recorded_at_values));
        let recorded_by_field: Arc<dyn Array> = Arc::new(StringArray::from(recorded_by_values));

        let parquet_writer = RecordBatch::try_from_iter([
            ("id", id_field),
            ("event", event_field),
            ("recorded_at", recorded_at_field),
            ("recorded_by", recorded_by_field),
        ])?;

        let mut buffer = Vec::<u8>::new();
        let mut writer = ArrowWriter::try_new(&mut buffer, parquet_writer.schema(), None)?;

        writer.write(&parquet_writer)?;
        writer.close()?;

        self.buffer.extend_from_slice(&buffer);

        Ok(0)
    }
}
