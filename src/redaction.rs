//! Shared redaction helpers for CLI previews, errors, and traces.

use serde_json::Value;

pub const REDACTED: &str = "<redacted>";

pub fn is_secret_name(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
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

pub fn redact_header(header: &str) -> String {
    let name = header
        .split_once(':')
        .map(|(name, _)| name)
        .unwrap_or(header);
    if is_secret_name(name) {
        format!("{}: {REDACTED}", name.trim())
    } else {
        header.to_string()
    }
}

pub fn redact_set_value(value: &str) -> String {
    let key = value.split_once('=').map(|(key, _)| key).unwrap_or(value);
    if is_secret_name(key) {
        format!("{key}={REDACTED}")
    } else {
        value.to_string()
    }
}

pub fn redact_json_value(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                if is_secret_name(key) {
                    *value = Value::String(REDACTED.to_string());
                } else {
                    redact_json_value(value);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_json_value(item);
            }
        }
        _ => {}
    }
}
