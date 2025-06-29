use crate::{
    AppState,
    errors::ApplicationError,
    exporter::{self, Exporter},
    utilities::generate_uuid_v4,
};
use axum::{extract::State, http::StatusCode, response::IntoResponse};
use chrono::Utc;
use libsql::{Connection, params};
use std::sync::Arc;
use tracing::{Instrument, info_span};

pub async fn post_event(
    State(state): State<AppState>,
    payload: String,
) -> Result<impl IntoResponse, ApplicationError> {
    let json_payload = serde_json::from_str(&payload)
        .map_err(|e| ApplicationError::InvalidPayload(e.to_string()))?;

    state
        .validator
        .validate(&json_payload)
        .map_err(|e| ApplicationError::InvalidPayload(e.to_string()))?;

    let recorded_by = json_payload
        .get("appId")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ApplicationError::InvalidPayload("Missing 'recorded_by' field".to_string())
        })?;

    state
        .connection
        .execute(
            "INSERT INTO events (id, recorded_at, recorded_by, event) VALUES (?1, ?2, ?3, json(?4))",
            params!(
                generate_uuid_v4(),
                Utc::now().to_rfc3339(),
                recorded_by,
                payload
            ),
        )
        .instrument(info_span!("insert_event"))
        .await?;

    Ok((StatusCode::ACCEPTED, String::new()))
}

pub async fn get_metrics(
    State((connection, instance_id)): State<(Arc<Connection>, String)>,
) -> Result<impl IntoResponse, ApplicationError> {
    let mut exporter = exporter::prometheus::PrometheusExporter {
        buffer: &mut String::new(),
    };
    exporter.publish(Some(instance_id), connection).await?;
    Ok((StatusCode::OK, exporter.buffer.clone()))
}
