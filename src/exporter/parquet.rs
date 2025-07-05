mod serializer;

use crate::{
    exporter::Exporter,
    storage::{EventSerializer, google_storage::GoogleStorageClient, memory::flush_since},
};
use chrono::{DateTime, Utc};
use serializer::{ParqetSerializer, VERSION};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tracing::info;

pub struct ParquetExporter {
    #[allow(dead_code)]
    pub last_export_at: DateTime<Utc>,
}

impl Exporter for ParquetExporter {
    async fn publish(
        &mut self,
        _exporter_identifier: Option<String>,
        source: Arc<libsql::Connection>,
    ) -> anyhow::Result<usize> {
        info!("Starting parquet export");

        let event_records = flush_since(source.clone(), self.last_export_at).await?;
        let (buffer, row_count) = ParqetSerializer.to_bytes(&event_records)?;

        if row_count > 0 {
            let mut client = GoogleStorageClient::new()?;
            let now = SystemTime::now();
            let duration = now.duration_since(UNIX_EPOCH)?;
            let micros = duration.as_micros();

            let filename = format!("{}/{}", VERSION, &micros.to_string());

            client
                .upload_binary_data(&filename, &buffer, Some("application/vnd.apache.parquet"))
                .await?;
        }

        info!("Parquet export completed successfully, exported {row_count} rows");

        Ok(row_count)
    }
}
