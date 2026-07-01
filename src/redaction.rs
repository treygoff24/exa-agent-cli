//! Shared redaction helpers for secret-named input validation and one-time-secret capture.

use serde_json::Value;

pub const REDACTED: &str = "<redacted>";

pub fn is_secret_name(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    if matches!(n.as_str(), "tokensnum" | "tokens_num" | "tokens-num") {
        return false;
    }
    n.contains("authorization")
        || n.contains("api-key")
        || n.contains("api_key")
        || n.contains("apikey")
        || n.contains("service-key")
        || n.contains("service_key")
        || n.contains("servicekey")
        || n.contains("access-key")
        || n.contains("access_key")
        || n.contains("accesskey")
        || n.contains("token")
        || n.contains("secret")
        || n.contains("password")
}

/// Replaces a top-level response field with [`REDACTED`] after one-time secret capture.
pub fn redact_named_field(value: &mut Value, field: &str) {
    let Value::Object(fields) = value else {
        return;
    };
    if fields.contains_key(field) {
        fields.insert(field.to_string(), Value::String(REDACTED.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_secret_names() {
        for name in [
            "authorization",
            "api-key",
            "api_key",
            "apikey",
            "service-key",
            "service_key",
            "servicekey",
            "access-key",
            "access_key",
            "accesskey",
            "token",
            "secret",
            "password",
        ] {
            assert!(is_secret_name(name), "{name}");
            assert!(is_secret_name(&name.to_uppercase()), "{name}");
        }

        assert!(!is_secret_name("limit"));
        assert!(!is_secret_name("query"));
        assert!(!is_secret_name("tokensNum"));
    }

    #[test]
    fn redact_named_field_replaces_only_target_field() {
        let mut value = json!({
            "id": "key_1",
            "apiKey": "SUPER_SECRET_XYZ",
            "name": "ci-key"
        });
        redact_named_field(&mut value, "apiKey");
        assert_eq!(value["id"], "key_1");
        assert_eq!(value["name"], "ci-key");
        assert_eq!(value["apiKey"], REDACTED);
    }

    #[test]
    fn redact_named_field_noop_when_missing_or_not_object() {
        let mut missing = json!({ "id": "key_1" });
        redact_named_field(&mut missing, "apiKey");
        assert_eq!(missing["id"], "key_1");
        assert!(missing.get("apiKey").is_none());

        let mut array = json!(["apiKey"]);
        redact_named_field(&mut array, "apiKey");
        assert_eq!(array, json!(["apiKey"]));
    }
}
