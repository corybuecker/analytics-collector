mod schema;

use crate::exporter::Exporter;
use chrono::{DateTime, Utc};
use parquet::arrow::ArrowWriter;
use schema::generate_record_batch;
use std::sync::Arc;
use tracing::{debug, info};

pub struct ParquetExporter<'a> {
    pub buffer: &'a mut Vec<u8>,
    #[allow(dead_code)]
    pub last_export_at: DateTime<Utc>,
}

impl Exporter for ParquetExporter<'_> {
    async fn publish(
        &mut self,
        _exporter_identifier: Option<String>,
        source: Arc<libsql::Connection>,
    ) -> anyhow::Result<usize> {
        info!("Starting parquet export");

        let (record_batch, row_count) = generate_record_batch(source.clone()).await?;

        let mut buffer = Vec::<u8>::new();
        let mut writer = ArrowWriter::try_new(&mut buffer, record_batch.schema(), None)?;

        writer.write(&record_batch)?;
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
