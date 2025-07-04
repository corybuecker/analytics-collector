#[cfg(feature = "export-parquet")]
pub mod parquet;
#[cfg(feature = "export-postgres")]
pub mod postgresql;
pub mod prometheus;

use anyhow::Result;
use std::sync::Arc;

pub trait Exporter {
    async fn publish(&mut self, source: Arc<libsql::Connection>) -> Result<usize>;
}
