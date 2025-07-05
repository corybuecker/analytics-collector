#[cfg(feature = "export-parquet")]
pub mod google_storage;
pub mod memory;

#[cfg(feature = "export-parquet")]
use anyhow::Result;

#[cfg(feature = "export-parquet")]
use memory::EventRecord;

pub const SCHEMA: &str = r#"
CREATE TABLE events (
    id TEXT PRIMARY KEY NOT NULL,
    recorded_at TIMESTAMP WITH TIME ZONE NOT NULL,
    recorded_by TEXT NOT NULL,
    event JSONB NOT NULL
);
"#;

#[cfg(feature = "export-parquet")]
pub trait EventSerializer {
    fn to_bytes<'a>(
        &self,
        event_records: impl IntoIterator<Item = &'a EventRecord>,
    ) -> Result<(Vec<u8>, usize)>;
}
