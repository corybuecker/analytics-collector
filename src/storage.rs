pub mod google_storage;
pub mod memory;

pub const SCHEMA: &str = r#"
CREATE TABLE events (
    id TEXT PRIMARY KEY NOT NULL,
    recorded_at TIMESTAMP WITH TIME ZONE NOT NULL,
    recorded_by TEXT NOT NULL,
    event JSONB NOT NULL
);
"#;
