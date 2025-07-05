use super::Exporter;
use anyhow::Result;
use chrono::{DateTime, Utc};
use libsql::params;
use rust_database_common::{Client, DatabasePool};
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

    async fn fetch_latest_recorded_at(&self, client: &Client) -> Option<DateTime<Utc>> {
        let latest_recorded_at: Option<String> = match client
            .query_opt("SELECT MAX(recorded_at) FROM events", &[])
            .await
        {
            Ok(opt_row) => opt_row.and_then(|row| row.get::<_, Option<String>>(0)),
            Err(e) => {
                error!("Failed to query latest recorded_at from postgres: {}", e);
                None
            }
        };
        latest_recorded_at
            .as_ref()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
    }

    async fn fetch_new_events(
        &self,
        memory_connection: &libsql::Connection,
        latest_recorded_at_dt: Option<&DateTime<Utc>>,
    ) -> Vec<(String, String, String, String)> {
        let (query, params) = if let Some(latest_dt) = latest_recorded_at_dt {
            (
                "SELECT id, recorded_at, recorded_by, event FROM events where recorded_at > ?"
                    .to_string(),
                params![latest_dt.to_rfc3339()],
            )
        } else {
            (
                "SELECT id, recorded_at, recorded_by, event FROM events".to_string(),
                params!([]),
            )
        };

        let mut stmt = match memory_connection.prepare(&query).await {
            Ok(stmt) => stmt,
            Err(e) => {
                error!("Failed to prepare statement: {}", e);
                return Vec::new();
            }
        };
        let mut rows: libsql::Rows = match stmt.query(params).await {
            Ok(rows) => rows,
            Err(e) => {
                error!("Failed to query events from memory db: {}", e);
                return Vec::new();
            }
        };
        let mut events = Vec::new();
        loop {
            match rows.next().await {
                Ok(Some(row)) => {
                    let id: String = row.get(0).unwrap_or_default();
                    let recorded_at: String = row.get(1).unwrap_or_default();
                    let recorded_by: String = row.get(2).unwrap_or_default();
                    let event: String = row.get(3).unwrap_or_default();

                    events.push((id, recorded_at, recorded_by, event));
                }
                Ok(None) => break,
                Err(e) => {
                    error!("Failed to read row: {}", e);
                    break;
                }
            }
        }
        events
    }

    async fn batch_insert_events(
        &self,
        client: &Client,
        events: &[(String, String, String, String)],
    ) {
        let batch_size = 100;
        for chunk in events.chunks(batch_size) {
            let mut values = Vec::new();
            let mut params: Vec<&(dyn rust_database_common::ToSql + Sync)> = Vec::new();
            for (i, (id, recorded_at, recorded_by, event)) in chunk.iter().enumerate() {
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
                params.push(recorded_by);
                params.push(event);
            }
            let query = format!(
                "INSERT INTO events (id, recorded_at, recorded_by, event) VALUES {} ON CONFLICT (id) DO NOTHING",
                values.join(", ")
            );
            if let Err(e) = client.execute(query.as_str(), &params).await {
                error!("Failed to batch insert events into postgres: {}", e);
            }
        }
    }
}

impl Exporter for PostgresqlExporter {
    async fn publish(&mut self, memory_connection: Arc<libsql::Connection>) -> Result<usize> {
        if !self.enabled {
            tracing::info!("PostgreSQL exporter is disabled, skipping flush.");
            return Ok(0);
        }

        let client: rust_database_common::Client = self
            .database_pool
            .clone()
            .expect("could not get database connection")
            .get_client()
            .await?;

        let latest_recorded_at_dt = self.fetch_latest_recorded_at(&client).await;
        debug!("Latest recorded_at: {:?}", latest_recorded_at_dt);

        let events = self
            .fetch_new_events(&memory_connection, latest_recorded_at_dt.as_ref())
            .await;

        if events.is_empty() {
            return Ok(0);
        }

        self.batch_insert_events(&client, &events).await;
        info!("Flushed {} events to PostgreSQL", events.len());
        Ok(events.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{storage::memory, utilities::generate_uuid_v4};
    use chrono::Days;
    use libsql::Connection;
    use tokio;

    async fn setup_memory_db() -> Arc<Connection> {
        let conn = memory::initialize().await.unwrap();
        Arc::new(conn)
    }

    async fn insert_event(
        conn: &Connection,
        id: &str,
        recorded_at: &str,
        recorded_by: &str,
        event: &str,
    ) {
        conn.execute(
            "INSERT INTO events (id, recorded_at, recorded_by, event) VALUES (?, ?, ?, ?)",
            libsql::params![id, recorded_at, recorded_by, event],
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_publish_flushes_events_to_postgres() {
        let recorded_by = generate_uuid_v4();
        let recorded_by = recorded_by.as_str();

        let recorded_at = Utc::now()
            .checked_add_days(Days::new(1))
            .unwrap()
            .to_rfc3339();
        let recorded_at = recorded_at.as_str();

        // Set up memory db and insert events
        let memory_conn = setup_memory_db().await;
        insert_event(
            &memory_conn,
            &generate_uuid_v4(),
            recorded_at,
            recorded_by,
            "event1",
        )
        .await;
        insert_event(
            &memory_conn,
            &generate_uuid_v4(),
            recorded_at,
            recorded_by,
            "event2",
        )
        .await;

        let mut exporter = PostgresqlExporter::build().await.unwrap();
        assert!(exporter.enabled);

        // Clean up events table in Postgres before test
        let pool = exporter.database_pool.as_ref().unwrap();
        let client = pool.get_client().await.unwrap();
        client
            .execute(
                "DELETE FROM events WHERE recorded_by IN ($1)",
                &[&recorded_by],
            )
            .await
            .unwrap();

        // Publish events
        let count = exporter.publish(memory_conn.clone()).await.unwrap();
        assert_eq!(count, 2);

        // Check events in Postgres
        let rows = client
            .query(
                "SELECT id, recorded_at, event FROM events WHERE recorded_by = $1 ORDER BY event",
                &[&recorded_by],
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
        let count = exporter.publish(memory_conn.clone()).await.unwrap();
        assert_eq!(count, 0);
    }
}
