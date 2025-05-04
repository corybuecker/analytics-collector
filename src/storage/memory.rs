use anyhow::Result;
use libsql::{Builder, Connection};

pub const SCHEMA: &str = r#"
CREATE TABLE events (
    ts TIMESTAMP WITH TIME ZONE NOT NULL,
    event JSONB NOT NULL
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
