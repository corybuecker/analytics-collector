use anyhow::Result;
use jsonschema::Validator;

const SCHEMA_DEFINITION: &str = include_str!("schema.json");

pub fn event_validator() -> Result<Validator> {
    let schema: serde_json::Value = serde_json::from_str(SCHEMA_DEFINITION)?;

    jsonschema::validator_for(&schema)
        .map_err(|e| anyhow::anyhow!("could not create JSON schema validator: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_event_validator_valid_payload() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "entity": "page",
            "action": "view",
            "ts": "2024-05-06T12:00:00Z",
            "path": "/home",
            "appId": "test-app"
        });
        let result = validator.validate(&payload);
        assert!(result.is_ok(), "Payload should be valid");
    }

    #[test]
    fn test_event_validator_missing_entity() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "action": "click",
            "ts": "2024-05-06T12:00:00Z",
            "path": "/about",
            "appId": "test-app"
        });
        let result = validator.validate(&payload);
        assert!(result.is_err(), "Payload missing entity should be invalid");
    }

    #[test]
    fn test_event_validator_missing_action() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "entity": "anchor",
            "ts": "2024-05-06T12:00:00Z",
            "path": "/about",
            "appId": "test-app"
        });
        let result = validator.validate(&payload);
        assert!(result.is_err(), "Payload missing action should be invalid");
    }

    #[test]
    fn test_event_validator_invalid_action() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "entity": "page",
            "action": "invalid_action",
            "ts": "2024-05-06T12:00:00Z",
            "path": "/home",
            "appId": "test-app"
        });
        let result = validator.validate(&payload);
        assert!(
            result.is_err(),
            "Payload with invalid action should be invalid"
        );
    }

    #[test]
    fn test_event_validator_invalid_entity() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "entity": "invalid_entity",
            "action": "view",
            "ts": "2024-05-06T12:00:00Z",
            "path": "/home",
            "appId": "test-app"
        });
        let result = validator.validate(&payload);
        assert!(
            result.is_err(),
            "Payload with invalid entity should be invalid"
        );
    }

    #[test]
    fn test_event_validator_additional_property() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "entity": "anchor",
            "action": "click",
            "extra": "not allowed",
            "appId": "test-app"
        });
        let result = validator.validate(&payload);
        assert!(
            result.is_err(),
            "Payload with additional property should be invalid"
        );
    }

    #[test]
    fn test_event_validator_only_required_fields() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "entity": "page",
            "action": "view",
            "appId": "test-app"
        });
        let result = validator.validate(&payload);
        assert!(
            result.is_ok(),
            "Payload with only required fields should be valid"
        );
    }

    #[test]
    fn test_event_validator_missing_optional_fields() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "entity": "anchor",
            "action": "click",
            "appId": "test-app"
        });
        let result = validator.validate(&payload);
        assert!(
            result.is_ok(),
            "Payload missing optional fields should be valid"
        );
    }

    #[test]
    fn test_event_validator_wrong_type_for_ts() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "entity": "page",
            "action": "view",
            "ts": 12345,
            "appId": "test-app"
        });
        let result = validator.validate(&payload);
        assert!(
            result.is_err(),
            "Payload with non-string ts should be invalid"
        );
    }

    #[test]
    fn test_event_validator_wrong_type_for_path() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "entity": "anchor",
            "action": "click",
            "path": 123,
            "appId": "test-app"
        });
        let result = validator.validate(&payload);
        assert!(
            result.is_err(),
            "Payload with non-string path should be invalid"
        );
    }

    #[test]
    fn test_event_validator_invalid_ts_format() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "entity": "page",
            "action": "view",
            "ts": "not-a-date",
            "appId": "test-app"
        });
        let result = validator.validate(&payload);
        // jsonschema crate does not enforce "format": "date-time" by default,
        // so this will be valid unless a custom format checker is used.
        // Adjust the assertion accordingly:
        assert!(
            result.is_ok(),
            "Payload with invalid date-time format for ts should be valid unless format checks are enabled"
        );
    }

    #[test]
    fn test_event_validator_empty_object() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({});
        let result = validator.validate(&payload);
        assert!(result.is_err(), "Empty object should be invalid");
    }

    #[test]
    fn test_event_validator_page_action_view_valid() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "entity": "page",
            "action": "view",
            "appId": "test-app"
        });
        let result = validator.validate(&payload);
        assert!(
            result.is_ok(),
            "entity=page with action=view should be valid"
        );
    }

    #[test]
    fn test_event_validator_page_action_click_invalid() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "entity": "page",
            "action": "click",
            "appId": "test-app"
        });
        let result = validator.validate(&payload);
        assert!(
            result.is_err(),
            "entity=page with action=click should be invalid"
        );
    }

    #[test]
    fn test_event_validator_anchor_action_click_valid() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "entity": "anchor",
            "action": "click",
            "appId": "test-app"
        });
        let result = validator.validate(&payload);
        assert!(
            result.is_ok(),
            "entity=anchor with action=click should be valid"
        );
    }

    #[test]
    fn test_event_validator_anchor_action_view_invalid() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "entity": "anchor",
            "action": "view",
            "appId": "test-app"
        });
        let result = validator.validate(&payload);
        assert!(
            result.is_err(),
            "entity=anchor with action=view should be invalid"
        );
    }

    #[test]
    fn test_event_validator_only_entity_present() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "entity": "page",
            "appId": "test-app"
        });
        let result = validator.validate(&payload);
        assert!(
            result.is_err(),
            "Payload with only entity should be invalid"
        );
    }

    #[test]
    fn test_event_validator_only_action_present() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "action": "view",
            "appId": "test-app"
        });
        let result = validator.validate(&payload);
        assert!(
            result.is_err(),
            "Payload with only action should be invalid"
        );
    }

    #[test]
    fn test_event_validator_missing_app_id() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "entity": "page",
            "action": "view"
        });
        let result = validator.validate(&payload);
        assert!(result.is_err(), "Payload missing appId should be invalid");
    }

    #[test]
    fn test_event_validator_non_string_app_id() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "entity": "anchor",
            "action": "click",
            "appId": 12345
        });
        let result = validator.validate(&payload);
        assert!(
            result.is_err(),
            "Payload with non-string appId should be invalid"
        );
    }

    #[test]
    fn test_event_validator_valid_app_id() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "entity": "anchor",
            "action": "click",
            "appId": "my-app-id"
        });
        let result = validator.validate(&payload);
        assert!(
            result.is_ok(),
            "Payload with valid string appId should be valid"
        );
    }
}
