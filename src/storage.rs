pub mod memory;

pub const SCHEMA: &str = r#"
CREATE TABLE events (
    id TEXT PRIMARY KEY NOT NULL,
    recorded_at TIMESTAMP WITH TIME ZONE NOT NULL,
    event TEXT NOT NULL
);
"#;
