use super::Exporter;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_postgres::Client;

pub struct PostgresqlExporter;

impl Exporter<Arc<RwLock<Client>>> for PostgresqlExporter {
    async fn publish(
        &self,
        _app_id: String,
        memory_connection: Arc<libsql::Connection>,
        postgres_client: Arc<RwLock<Client>>,
    ) -> Result<()> {
        let mut stmt = match memory_connection
            .prepare("SELECT id, recorded_at, event FROM events")
            .await
        {
            Ok(stmt) => stmt,
            Err(e) => {
                tracing::error!("Failed to prepare statement: {}", e);
                return Ok(());
            }
        };
        let mut rows = match stmt.query(libsql::params![]).await {
            Ok(rows) => rows,
            Err(e) => {
                tracing::error!("Failed to query events from memory db: {}", e);
                return Ok(());
            }
        };
        let mut events = Vec::new();
        loop {
            match rows.next().await {
                Ok(Some(row)) => {
                    let id: String = row.get(0).unwrap_or_default();
                    let recorded_at: String = row.get(1).unwrap_or_default();
                    let event: String = row.get(2).unwrap_or_default();
                    events.push((id, recorded_at, event));
                }
                Ok(None) => break,
                Err(e) => {
                    tracing::error!("Failed to read row: {}", e);
                    break;
                }
            }
        }
        if events.is_empty() {
            return Ok(());
        }
        // Insert events into PostgreSQL
        let client = postgres_client.read().await;
        for (id, recorded_at, event) in &events {
            if let Err(e) = client
                .execute(
                    "INSERT INTO events (id, recorded_at, event) VALUES ($1, $2, $3) ON CONFLICT (id) DO NOTHING",
                    &[id, recorded_at, event],
                )
                .await
            {
                tracing::error!("Failed to insert event into postgres: {}", e);
            }
        }
        tracing::info!("Flushed {} events to PostgreSQL", events.len());

        Ok(())
    }
}
