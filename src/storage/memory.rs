use anyhow::Result;
use libsql::{Builder, Connection};

use super::SCHEMA;

pub async fn initialize() -> Result<Connection> {
    let memory_database = Builder::new_local(":memory:")
        .build()
        .await
        .expect("failed to create in-memory database");

    let connection = memory_database.connect()?;
    connection.execute(SCHEMA, ()).await?;

    Ok(connection)
}
