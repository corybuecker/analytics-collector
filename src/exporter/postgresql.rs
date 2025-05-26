use super::Exporter;
use anyhow::Result;
use std::{sync::Arc, time::Duration};
use tokio::{spawn, sync::RwLock, time::sleep_until};
use tokio_postgres::{Client, NoTls, connect};
use tracing::{error, info};

#[derive(Debug, Clone)]
pub struct PostgresqlExporter {
    pub client: Option<Arc<RwLock<Client>>>,
    pub enabled: bool,
}

impl PostgresqlExporter {
    pub async fn build() -> Self {
        let database_url = std::env::var("DATABASE_URL").ok();

        if database_url.is_none() {
            info!("PostgreSQL exporter is disabled, skipping initialization.");
            return Self {
                client: None,
                enabled: false,
            };
        }

        let database_url = database_url.expect("DATABASE_URL must be set");

        let (client, _connection) = connect(&database_url, NoTls)
            .await
            .expect("Failed to connect to PostgreSQL");
        let client = Arc::new(RwLock::new(client));

        let moved_client = client.clone();
        spawn(async move { database_connection_handler(moved_client).await });

        Self {
            client: Some(client),
            enabled: true,
        }
    }
}

async fn database_connection_handler(client: Arc<RwLock<Client>>) {
    // Get the database URL from the environment variable
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL not set");

    loop {
        // Try to connect to the database
        let (replacement_client, connection) = match connect(&database_url, NoTls).await {
            Ok((client, connection)) => {
                info!("Connected to database");
                (client, connection)
            }
            // If connection fails, log the error and retry after 5 seconds
            Err(e) => {
                tracing::error!("Failed to connect to database: {}", e);
                sleep_until(tokio::time::Instant::now() + Duration::from_secs(5)).await;
                continue;
            }
        };

        // Replace the current client with the new one.
        // Acquire a write lock on the Arc-wrapped RwLock<Client> to ensure exclusive access,
        // so that no other task is reading or writing to the client while we update it.
        let mut guard = client.write().await;

        // Overwrite the existing client with the newly established replacement_client.
        // This allows the rest of the application to transparently use the new connection
        // without needing to restart or reinitialize any consumers of the client.
        *guard = replacement_client;

        // Explicitly drop the guard to release the write lock as soon as possible,
        // allowing other tasks to acquire the lock and use the updated client.
        drop(guard);

        // Wait for the connection to finish, log errors if any, and loop to reconnect
        if let Err(e) = connection.await {
            error!("Connection error: {}", e);
            continue;
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
        // Insert events into PostgreSQL
        let client = self.client.clone().expect("PostgreSQL client is unexpectedly absent. Ensure the client is properly initialized.");
        let client = client.read().await;
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
