use super::Exporter;
use anyhow::Result;
use rust_database_common::DatabasePool;
use std::sync::Arc;
use tracing::{debug, error, info};

#[derive(Debug, Clone)]
pub struct PostgresqlExporter {
    pub database_pool: Option<DatabasePool>,
    pub enabled: bool,
}

impl PostgresqlExporter {
    pub async fn build() -> Result<Self> {
        let database_url = std::env::var("DATABASE_URL").ok();

        match database_url {
            Some(url) => {
                let mut database_pool = DatabasePool::new(url);
                database_pool.connect().await?;
                debug!("PostgreSQL exporter initialized with live database");
                Ok(Self {
                    database_pool: Some(database_pool),
                    enabled: true,
                })
            }
            None => Ok(Self {
                database_pool: None,
                enabled: false,
            }),
        }
    }
}

impl Exporter for PostgresqlExporter {
    async fn publish(
        &mut self,
        _app_id: String,
        memory_connection: Arc<libsql::Connection>,
    ) -> Result<usize> {
        if !self.enabled {
            tracing::info!("PostgreSQL exporter is disabled, skipping flush.");
            return Ok(0);
        }

        let mut stmt = match memory_connection
            .prepare("SELECT id, recorded_at, event FROM events")
            .await
        {
            Ok(stmt) => stmt,
            Err(e) => {
                error!("Failed to prepare statement: {}", e);
                return Ok(0);
            }
        };
        let mut rows = match stmt.query(libsql::params![]).await {
            Ok(rows) => rows,
            Err(e) => {
                error!("Failed to query events from memory db: {}", e);
                return Ok(0);
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
                    error!("Failed to read row: {}", e);
                    break;
                }
            }
        }
        if events.is_empty() {
            return Ok(0);
        }

        let client = self
            .database_pool
            .clone()
            .expect("could not get database connection")
            .get_client()
            .await?;

        for (id, recorded_at, event) in &events {
            if let Err(e) = client
                .execute(
                    "INSERT INTO events (id, recorded_at, event) VALUES ($1, $2, $3) ON CONFLICT (id) DO NOTHING",
                    &[id, recorded_at, event],
                )
                .await
            {
                error!("Failed to insert event into postgres: {}", e);
            }
        }

        info!("Flushed {} events to PostgreSQL", events.len());

        Ok(events.len())
    }
}
