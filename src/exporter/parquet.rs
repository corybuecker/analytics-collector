mod schema;
mod serializer;

use crate::{
    exporter::Exporter,
    storage::{EventSerializer, memory::flush_since},
};
use chrono::{DateTime, Utc};
use serializer::ParqetSerializer;
use std::sync::Arc;
use tracing::info;

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
        let event_records = flush_since(source.clone(), self.last_export_at).await?;
        let (buffer, row_count) = ParqetSerializer.to_bytes(&event_records)?;

        self.buffer.extend_from_slice(&buffer);

        info!(
            "Parquet export completed successfully, exported {} rows",
            row_count
        );
        Ok(row_count)
    }
}
