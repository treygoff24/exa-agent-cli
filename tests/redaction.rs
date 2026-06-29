use exa_agent_cli::error::{CliError, Diag};
use exa_agent_cli::output::envelope::ErrorEnvelope;
use exa_agent_cli::redaction::{
    is_secret_name, redact_header, redact_json_value, redact_set_value, scrub_json_value,
    scrub_text,
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
    assert_eq!(
        redact_header("x-trace-id: exa-secret-header-1234"),
        "x-trace-id: <redacted>"
    );
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

#[test]
fn scrubs_secret_shaped_values_in_text_and_json_values() {
    let uuid_secret = "11111111-2222-3333-4444-555555555555";
    assert_eq!(scrub_text(uuid_secret), "<redacted>");
    assert_eq!(
        scrub_text("exa-agent contents <inputs> --chunk-size 100"),
        "exa-agent contents <inputs> --chunk-size 100"
    );
    assert_eq!(
        scrub_text("bad https://exa-secret-url-1234/path"),
        "bad https://<redacted>/path"
    );

    let mut body = json!({ "query": "exa-secret-query-1234", "normal": "keep-me" });
    redact_json_value(&mut body);
    assert_eq!(body["query"], "<redacted>");
    assert_eq!(body["normal"], "keep-me");
}

#[test]
fn error_envelope_scrubs_message_details_and_suggestion() {
    let err = CliError::Usage(
        Diag::new("invalid_value", "bad value exa-secret-message-1234")
            .with_details(json!({
                "candidate": "sk-exa-detail-1234",
                "nested": ["11111111-2222-3333-4444-555555555555"]
            }))
            .with_suggestion("retry with service-secret-suggestion-1234"),
    );
    let json = ErrorEnvelope::from_error(&err).to_json();
    let serialized = serde_json::to_string(&json).unwrap();
    assert!(!serialized.contains("exa-secret-message-1234"));
    assert!(!serialized.contains("sk-exa-detail-1234"));
    assert!(!serialized.contains("11111111-2222-3333-4444-555555555555"));
    assert!(!serialized.contains("service-secret-suggestion-1234"));
    assert!(serialized.contains("<redacted>"));
}

#[test]
fn scrub_json_preserves_contract_identifiers() {
    let mut value = json!({
        "schema": "exa.cli.doctor.v1",
        "id": "service-key.scope",
        "status": "fail",
        "category": "auth",
        "message": "bad sk-exa-message-1234",
        "api_key": "sk-exa-field-1234",
        "details": {
            "code": "invalid_value",
            "candidate": "11111111-2222-3333-4444-555555555555"
        }
    });

    scrub_json_value(&mut value);

    assert_eq!(value["schema"], "exa.cli.doctor.v1");
    assert_eq!(value["id"], "service-key.scope");
    assert_eq!(value["status"], "fail");
    assert_eq!(value["category"], "auth");
    assert_eq!(value["details"]["code"], "invalid_value");
    assert_eq!(value["message"], "bad <redacted>");
    assert_eq!(value["api_key"], "<redacted>");
    assert_eq!(value["details"]["candidate"], "<redacted>");
}
