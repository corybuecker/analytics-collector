pub mod memory;

pub const SCHEMA: &str = r#"
CREATE TABLE events (
    id TEXT PRIMARY KEY NOT NULL,
    recorded_at TEXT NOT NULL,
    event TEXT NOT NULL
);
"#;
