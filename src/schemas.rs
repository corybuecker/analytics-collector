use anyhow::Result;
use jsonschema::Validator;
use serde_json::json;

pub fn event_validator() -> Result<Validator> {
    let schema = json!(
        {
            "type": "object",
            "properties": {
              "ts": {
                "type": "string",
                "format": "date-time"
              },
              "event_name": {
                "type": "string"
              },
              "action": {
                "type": "string",
                "enum": ["page_view", "click"]
              },
              "path": {
                "type": "string"
              }
            },
            "required": ["event_name"],
            "additionalProperties": false
          }
    );

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
            "event_name": "signup",
            "ts": "2024-05-06T12:00:00Z",
            "action": "page_view"
        });
        let result = validator.validate(&payload);
        assert!(result.is_ok(), "Payload should be valid");
    }

    #[test]
    fn test_event_validator_missing_event_name() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "ts": "2024-05-06T12:00:00Z",
            "action": "click"
        });
        let result = validator.validate(&payload);
        assert!(
            result.is_err(),
            "Payload missing event_name should be invalid"
        );
    }

    #[test]
    fn test_event_validator_invalid_action() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "event_name": "signup",
            "action": "invalid_action"
        });
        let result = validator.validate(&payload);
        assert!(
            result.is_err(),
            "Payload with invalid action should be invalid"
        );
    }

    #[test]
    fn test_event_validator_additional_property() {
        let validator = event_validator().expect("validator should be created");
        let payload = json!({
            "event_name": "signup",
            "extra": "not allowed"
        });
        let result = validator.validate(&payload);
        assert!(
            result.is_err(),
            "Payload with additional property should be invalid"
        );
    }
}
