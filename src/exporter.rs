pub mod parquet;
pub mod postgresql;
pub mod prometheus;

use anyhow::Result;
use std::sync::Arc;

pub trait Exporter {
    async fn publish(
        &mut self,
        exporter_identifier: Option<String>,
        source: Arc<libsql::Connection>,
    ) -> Result<usize>;
}
