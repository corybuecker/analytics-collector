use anyhow::{Result, anyhow};
use arrow_array::{ArrayRef, RecordBatch, StringArray};
use arrow_schema::{Field, Schema, SchemaBuilder};
use std::sync::Arc;
use tracing::info;

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

pub fn generate_record_batch<'a>(
    event_records: impl IntoIterator<Item = &'a crate::storage::memory::EventRecord>,
) -> Result<(arrow_array::RecordBatch, usize)> {
    let mut id_values = Vec::<String>::new();

    let mut event_ts_values = Vec::<Option<String>>::new();
    let mut event_entity_values = Vec::<String>::new();
    let mut event_action_values = Vec::<String>::new();
    let mut event_path_values = Vec::<Option<String>>::new();
    let mut event_app_id_values = Vec::<String>::new();

    let mut recorded_at_values = Vec::<String>::new();
    let mut recorded_by_values = Vec::<Option<String>>::new();

    for event_record in event_records {
        id_values.push(event_record.id.clone());

        event_ts_values.push(event_record.event.ts.map(|t| t.to_rfc3339()));
        event_entity_values.push(event_record.event.entity.clone());
        event_action_values.push(event_record.event.action.clone());
        event_path_values.push(event_record.event.path.clone());
        event_app_id_values.push(event_record.event.app_id.clone());

        recorded_at_values.push(event_record.recorded_at.clone().to_rfc3339());
        recorded_by_values.push(event_record.recorded_by.clone());
    }

    let event_values = arrow_array::StructArray::from(vec![
        (
            Arc::new(arrow_schema::Field::new(
                "ts",
                arrow_schema::DataType::Utf8,
                true,
            )),
            (Arc::new(arrow_array::StringArray::from(event_ts_values)) as ArrayRef),
        ),
        (
            Arc::new(Field::new(
                "entity",
                arrow::datatypes::DataType::Utf8,
                false,
            )),
            Arc::new(StringArray::from(event_entity_values)),
        ),
        (
            Arc::new(Field::new(
                "action",
                arrow::datatypes::DataType::Utf8,
                false,
            )),
            Arc::new(StringArray::from(event_action_values)),
        ),
        (
            Arc::new(Field::new("path", arrow::datatypes::DataType::Utf8, true)),
            Arc::new(StringArray::from(event_path_values)),
        ),
        (
            Arc::new(Field::new(
                "app_id",
                arrow::datatypes::DataType::Utf8,
                false,
            )),
            Arc::new(StringArray::from(event_app_id_values)),
        ),
    ]);

    let row_count = id_values.len();

    info!("Processed {} total rows from database", row_count);

    Ok((
        RecordBatch::try_new(
            generate_schema(),
            vec![
                Arc::new(StringArray::from(id_values)),
                Arc::new(event_values),
                Arc::new(StringArray::from(recorded_at_values)),
                Arc::new(StringArray::from(recorded_by_values)),
            ],
        )
        .map_err(|e| anyhow!("{:?}", e))?,
        row_count,
    ))
}
