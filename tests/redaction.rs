use exa_agent_cli::redaction::{
    is_secret_name, redact_header, redact_json_value, redact_set_value,
};
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
}

#[test]
fn redacts_header_values() {
    assert_eq!(
        redact_header("Authorization: Bearer header-secret"),
        "Authorization: <redacted>"
    );
    assert_eq!(
        redact_header("x-exa-service-key: service-key-secret"),
        "x-exa-service-key: <redacted>"
    );
    assert_eq!(redact_header("x-trace-id: keep-me"), "x-trace-id: keep-me");
}

#[test]
fn redacts_set_values() {
    assert_eq!(
        redact_set_value("webhookSecret=set-secret"),
        "webhookSecret=<redacted>"
    );
    assert_eq!(
        redact_set_value("nested.api_key=set-secret"),
        "nested.api_key=<redacted>"
    );
    assert_eq!(redact_set_value("query=keep-me"), "query=keep-me");
}

#[test]
fn redacts_nested_json_body_fields() {
    let mut body = json!({
        "query": "keep-me",
        "token": "body-secret",
        "nested": {
            "password": "nested-secret",
            "name": "keep-name",
            "items": [
                { "access_key": "array-secret", "value": "keep-array" }
            ]
        }
    });

    redact_json_value(&mut body);

    assert_eq!(body["query"], "keep-me");
    assert_eq!(body["token"], "<redacted>");
    assert_eq!(body["nested"]["password"], "<redacted>");
    assert_eq!(body["nested"]["name"], "keep-name");
    assert_eq!(body["nested"]["items"][0]["access_key"], "<redacted>");
    assert_eq!(body["nested"]["items"][0]["value"], "keep-array");
}
