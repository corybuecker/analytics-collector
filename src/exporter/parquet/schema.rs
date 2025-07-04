use arrow_schema::{Field, Schema, SchemaBuilder};
use std::sync::Arc;

pub fn generate_schema() -> Arc<Schema> {
    let mut builder = SchemaBuilder::new();

    let event_fields = vec![
        Field::new("ts", arrow::datatypes::DataType::Utf8, true),
        Field::new("entity", arrow::datatypes::DataType::Utf8, false),
        Field::new("action", arrow::datatypes::DataType::Utf8, false),
        Field::new("path", arrow::datatypes::DataType::Utf8, true),
        Field::new("app_id", arrow::datatypes::DataType::Utf8, false),
    ];

    builder.push(Field::new("id", arrow::datatypes::DataType::Utf8, false));
    builder.push(Field::new_struct("event", event_fields, false));
    builder.push(Field::new(
        "recorded_at",
        arrow::datatypes::DataType::Utf8,
        false,
    ));
    builder.push(Field::new(
        "recorded_by",
        arrow::datatypes::DataType::Utf8,
        true,
    ));

    Arc::new(builder.finish())
}
