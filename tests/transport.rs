//! Transport seam tests (Wave 1D): fake transport, retry policy, header refusal.

use clap::Parser;
use exa_agent_cli::auth::{self, CredentialInput, NoopKeyring};
use exa_agent_cli::cli::GlobalArgs;
use exa_agent_cli::error::CliError;
use exa_agent_cli::transport::{
    build_url, classify_http_status, execute_raw, parse_user_headers, send_with_retry,
    FakeTransport, HttpRequest, SendOptions,
};

#[test]
fn user_headers_allow_non_secret_and_refuse_auth() {
    let ok = parse_user_headers(&["X-Trace: abc".into()]).unwrap();
    assert_eq!(ok, vec![("X-Trace".into(), "abc".into())]);
    let err = parse_user_headers(&["Authorization: Bearer x".into()]).unwrap_err();
    assert_eq!(err.diag().code, "invalid_flag_combination");
}

#[test]
fn build_url_percent_encodes_query_values() {
    let url = build_url(
        "https://api.exa.ai",
        "/search",
        &[("q".into(), "hello world".into())],
    )
    .unwrap();
    assert_eq!(url, "https://api.exa.ai/search?q=hello%20world");
}

#[test]
fn execute_raw_injects_auth_and_serializes_body() {
    let fake = FakeTransport::default();
    fake.push_ok_json(200, r#"{"results":[{"title":"x"}]}"#);
    let globals = parse_globals(&["--api-key", "test-key-abcdef12"]);
    let cred = auth::resolve_api_credential(
        &CredentialInput {
            explicit: Some("test-key-abcdef12".into()),
            ..Default::default()
        },
        &NoopKeyring,
    )
    .unwrap();
    let out = execute_raw(
        &fake,
        "POST",
        "/search",
        &[],
        serde_json::json!({"query":"agents"}),
        &globals,
        &cred,
    )
    .unwrap();
    assert_eq!(out.response.status, 200);
    let req = &fake.recorded_requests()[0];
    assert!(req.headers.iter().any(|(k, _)| k == "x-api-key"));
    assert!(!req.headers.iter().any(|(k, _)| k == "Authorization"));
    assert!(req.body.is_some());
}

#[test]
fn execute_raw_allows_documented_get_with_body() {
    let fake = FakeTransport::default();
    fake.push_ok_json(200, r#"{"ok":true}"#);
    let globals = parse_globals(&["--api-key", "test-key-abcdef12"]);
    let cred = auth::resolve_api_credential(
        &CredentialInput {
            explicit: Some("test-key-abcdef12".into()),
            ..Default::default()
        },
        &NoopKeyring,
    )
    .unwrap();
    let out = execute_raw(
        &fake,
        "GET",
        "/search",
        &[],
        serde_json::json!({"query":"agents"}),
        &globals,
        &cred,
    )
    .unwrap();
    assert_eq!(out.response.status, 200);
    let req = &fake.recorded_requests()[0];
    assert_eq!(req.method, "GET");
    assert!(req.body.is_some());
}

#[test]
fn execute_raw_forwards_idempotency_key_header() {
    let fake = FakeTransport::default();
    fake.push_ok_json(200, r#"{"ok":true}"#);
    let globals = parse_globals(&[
        "--api-key",
        "test-key-abcdef12",
        "--idempotency-key",
        "idem-123",
    ]);
    let cred = auth::resolve_api_credential(
        &CredentialInput {
            explicit: Some("test-key-abcdef12".into()),
            ..Default::default()
        },
        &NoopKeyring,
    )
    .unwrap();
    execute_raw(
        &fake,
        "POST",
        "/agent/runs",
        &[],
        serde_json::json!({"prompt":"go"}),
        &globals,
        &cred,
    )
    .unwrap();
    let req = &fake.recorded_requests()[0];
    assert!(req
        .headers
        .iter()
        .any(|(k, v)| k == "Idempotency-Key" && v == "idem-123"));
}

#[test]
fn execute_raw_preserves_custom_content_type_header() {
    let fake = FakeTransport::default();
    fake.push_ok_json(200, r#"{"ok":true}"#);
    let globals = parse_globals(&[
        "--api-key",
        "test-key-abcdef12",
        "--header",
        "Content-Type: application/json-patch+json",
    ]);
    let cred = auth::resolve_api_credential(
        &CredentialInput {
            explicit: Some("test-key-abcdef12".into()),
            ..Default::default()
        },
        &NoopKeyring,
    )
    .unwrap();
    execute_raw(
        &fake,
        "POST",
        "/custom",
        &[],
        serde_json::json!([{"op":"replace","path":"/name","value":"x"}]),
        &globals,
        &cred,
    )
    .unwrap();
    let req = &fake.recorded_requests()[0];
    let content_types: Vec<_> = req
        .headers
        .iter()
        .filter(|(k, _)| k.eq_ignore_ascii_case("content-type"))
        .collect();
    assert_eq!(content_types.len(), 1);
    assert_eq!(content_types[0].1, "application/json-patch+json");
}

#[test]
fn post_with_idempotency_key_is_retried_on_503() {
    let fake = FakeTransport::default();
    fake.push_ok_json(503, "unavailable");
    fake.push_ok_json(200, r#"{"ok":true}"#);
    let req = HttpRequest {
        method: "POST".into(),
        url: "https://api.exa.ai/agent/runs".into(),
        headers: vec![("Idempotency-Key".into(), "idem-123".into())],
        body: Some(b"{}".to_vec()),
    };
    let opts = SendOptions {
        retry: 2,
        retry_after: false,
        idempotency_key: Some("idem-123".into()),
    };
    let (resp, retries) = send_with_retry(&fake, &req, &opts).unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(retries, 1);
    assert_eq!(fake.recorded_requests().len(), 2);
}

#[test]
fn options_is_supported_by_retry_model() {
    let fake = FakeTransport::default();
    fake.push_ok_json(503, "unavailable");
    fake.push_ok_json(200, r#"{"ok":true}"#);
    let req = HttpRequest {
        method: "OPTIONS".into(),
        url: "https://api.exa.ai/search".into(),
        headers: vec![],
        body: None,
    };
    let opts = SendOptions {
        retry: 2,
        retry_after: false,
        idempotency_key: None,
    };
    let (resp, retries) = send_with_retry(&fake, &req, &opts).unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(retries, 1);
}

#[test]
fn status_409_mentions_idempotency_conflict_when_body_does() {
    let err = classify_http_status(409, b"idempotency key reused", &[]);
    assert_eq!(err.diag().code, "idempotency_conflict");
}

#[test]
fn create_post_is_not_retried_without_idempotency_key() {
    let fake = FakeTransport::default();
    fake.push_ok_json(503, "unavailable");
    let req = HttpRequest {
        method: "POST".into(),
        url: "https://api.exa.ai/search".into(),
        headers: vec![],
        body: Some(b"{}".to_vec()),
    };
    let opts = SendOptions {
        retry: 2,
        retry_after: false,
        idempotency_key: None,
    };
    let err = send_with_retry(&fake, &req, &opts).unwrap_err();
    assert!(matches!(err, CliError::Upstream(_)));
    assert_eq!(fake.recorded_requests().len(), 1);
}

#[test]
fn get_is_retried_on_upstream_503() {
    let fake = FakeTransport::default();
    fake.push_ok_json(503, "unavailable");
    fake.push_ok_json(200, r#"{"ok":true}"#);
    let req = HttpRequest {
        method: "GET".into(),
        url: "https://api.exa.ai/health".into(),
        headers: vec![],
        body: None,
    };
    let opts = SendOptions {
        retry: 2,
        retry_after: false,
        idempotency_key: None,
    };
    let (resp, retries) = send_with_retry(&fake, &req, &opts).unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(retries, 1);
}

fn parse_globals(args: &[&str]) -> GlobalArgs {
    let argv: Vec<String> = std::iter::once("exa-agent")
        .chain(args.iter().copied())
        .chain(std::iter::once("capabilities"))
        .map(String::from)
        .collect();
    exa_agent_cli::cli::Cli::try_parse_from(argv)
        .expect("parse globals")
        .globals
}
