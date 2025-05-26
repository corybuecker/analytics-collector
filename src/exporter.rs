pub mod postgresql;
pub mod prometheus;

use anyhow::Result;
use std::sync::Arc;

pub trait Exporter {
    async fn publish(&mut self, app_id: String, source: Arc<libsql::Connection>) -> Result<usize>;
}
