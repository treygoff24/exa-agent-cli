use exa_agent_cli::redaction::{is_secret_name, redact_named_field, REDACTED};
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
    let mut body = json!({
        "id": "create-api-key_id",
        "apiKey": "registry_property_secret_create-api-key",
        "name": "keep-name"
    });

    redact_named_field(&mut body, "apiKey");

    assert_eq!(body["id"], "create-api-key_id");
    assert_eq!(body["name"], "keep-name");
    assert_eq!(body["apiKey"], REDACTED);
}
