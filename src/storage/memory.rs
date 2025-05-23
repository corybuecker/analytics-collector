use anyhow::Result;
use libsql::{Builder, Connection};

pub const SCHEMA: &str = r#"
CREATE TABLE events (
    id TEXT PRIMARY KEY NOT NULL,
    recorded_at TEXT NOT NULL,
    event TEXT NOT NULL
);
"#;

pub async fn initialize() -> Result<Connection> {
    let memory_database = Builder::new_local(":memory:")
        .build()
        .await
        .expect("failed to create in-memory database");

    let connection = memory_database.connect()?;
    connection.execute(SCHEMA, ()).await?;

    Ok(connection)
}
