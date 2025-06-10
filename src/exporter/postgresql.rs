use super::Exporter;
use anyhow::Result;
use chrono::{DateTime, Utc};
use libsql::params;
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
        app_id: String,
        memory_connection: Arc<libsql::Connection>,
    ) -> Result<usize> {
        if !self.enabled {
            tracing::info!("PostgreSQL exporter is disabled, skipping flush.");
            return Ok(0);
        }

        // Get the most recent recorded_at from the destination database
        let client = self
            .database_pool
            .clone()
            .expect("could not get database connection")
            .get_client()
            .await?;

        let latest_recorded_at: Option<String> = match client
            .query_opt(
                "SELECT MAX(recorded_at) FROM events WHERE app_id = $1",
                &[&app_id],
            )
            .await
        {
            Ok(opt_row) => opt_row.and_then(|row| row.get::<_, Option<String>>(0)),
            Err(e) => {
                error!("Failed to query latest recorded_at from postgres: {}", e);
                None
            }
        };
        let latest_recorded_at_dt: Option<DateTime<Utc>> = latest_recorded_at
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc));

        debug!(
            "Latest recorded_at for app_id {}: {:?}",
            app_id, latest_recorded_at_dt
        );

        // Prepare select statement with or without filter
        let (query, params) = if let Some(ref latest_dt) = latest_recorded_at_dt {
            (
                "SELECT id, recorded_at, event FROM events where recorded_at > ?".to_string(),
                params![latest_dt.to_rfc3339()],
            )
        } else {
            (
                "SELECT id, recorded_at, event FROM events".to_string(),
                params!([]),
            )
        };

        let mut stmt = match memory_connection.prepare(&query).await {
            Ok(stmt) => stmt,
            Err(e) => {
                error!("Failed to prepare statement: {}", e);
                return Ok(0);
            }
        };
        let mut rows: libsql::Rows = match stmt.query(params).await {
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

        // Batch insert
        let batch_size = 100;
        let local_app_id = app_id.clone();
        for chunk in events.chunks(batch_size) {
            // Build the VALUES part of the query dynamically
            let mut values = Vec::new();
            let mut params: Vec<&(dyn rust_database_common::ToSql + Sync)> = Vec::new();
            for (i, (id, recorded_at, event)) in chunk.iter().enumerate() {
                let base = i * 4;
                values.push(format!(
                    "(${}, ${}, ${}, ${})",
                    base + 1,
                    base + 2,
                    base + 3,
                    base + 4
                ));
                params.push(id);
                params.push(recorded_at);
                params.push(event);
                params.push(&local_app_id);
            }

            let query = format!(
                "INSERT INTO events (id, recorded_at, event, app_id) VALUES {} ON CONFLICT (id) DO NOTHING",
                values.join(", ")
            );
            if let Err(e) = client.execute(query.as_str(), &params).await {
                error!("Failed to batch insert events into postgres: {}", e);
            }
        }

        info!("Flushed {} events to PostgreSQL", events.len());

        Ok(events.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{storage::memory, utilities::generate_uuid_v4};
    use libsql::Connection;
    use tokio;

    async fn setup_memory_db() -> Arc<Connection> {
        let conn = memory::initialize().await.unwrap();
        Arc::new(conn)
    }

    async fn insert_event(conn: &Connection, id: &str, recorded_at: &str, event: &str) {
        conn.execute(
            "INSERT INTO events (id, recorded_at, event) VALUES (?, ?, ?)",
            libsql::params![id, recorded_at, event],
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_publish_flushes_events_to_postgres() {
        let app_id = generate_uuid_v4();

        // Set up memory db and insert events
        let memory_conn = setup_memory_db().await;
        insert_event(
            &memory_conn,
            &generate_uuid_v4(),
            "2024-01-01T00:00:00Z",
            "event1",
        )
        .await;
        insert_event(
            &memory_conn,
            &generate_uuid_v4(),
            "2024-01-01T01:00:00Z",
            "event2",
        )
        .await;

        let mut exporter = PostgresqlExporter::build().await.unwrap();
        assert!(exporter.enabled);

        // Clean up events table in Postgres before test
        let pool = exporter.database_pool.as_ref().unwrap();
        let client = pool.get_client().await.unwrap();
        client
            .execute("DELETE FROM events WHERE app_id IN ($1)", &[&app_id])
            .await
            .unwrap();

        // Publish events
        let count = exporter
            .publish(app_id.clone(), memory_conn.clone())
            .await
            .unwrap();
        assert_eq!(count, 2);

        // Check events in Postgres
        let rows = client
            .query(
                "SELECT id, recorded_at, event FROM events WHERE app_id = $1 ORDER BY event",
                &[&app_id],
            )
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get::<_, String>(2), "event1");
        assert_eq!(rows[1].get::<_, String>(2), "event2");
    }

    #[tokio::test]
    async fn test_publish_no_events() {
        let memory_conn = setup_memory_db().await;
        let mut exporter = PostgresqlExporter::build().await.unwrap();
        let count = exporter
            .publish("test_app".to_string(), memory_conn.clone())
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}
