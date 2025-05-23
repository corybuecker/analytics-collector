pub mod postgresql;
pub mod prometheus;

use anyhow::Result;
use std::sync::Arc;

pub trait Exporter<T> {
    async fn publish(
        &self,
        app_id: String,
        source: Arc<libsql::Connection>,
        destination: T,
    ) -> Result<()>;
}
