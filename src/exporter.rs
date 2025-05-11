pub mod prometheus;

use anyhow::Result;
use std::sync::Arc;

pub trait Exporter {
    async fn publish(&self, connection: Arc<libsql::Connection>, app_id: String) -> Result<String>;
}
