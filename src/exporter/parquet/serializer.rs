use anyhow::Result;
use parquet::arrow::ArrowWriter;
use tracing::debug;

use crate::storage::EventSerializer;

use super::schema::generate_record_batch;

pub struct ParqetSerializer;

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
