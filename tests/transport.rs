//! Transport seam tests (Wave 1D): fake transport, retry policy, header refusal.

use clap::Parser;
use exa_agent_cli::auth::{self, CredentialInput, NoopKeyring, Secret};
use exa_agent_cli::cli::GlobalArgs;
use exa_agent_cli::error::{CliError, Diag};
use exa_agent_cli::transport::{
    build_url, classify_http_status, contents_outcome, execute_raw, parse_user_headers, probe_auth,
    probe_connectivity, send_with_retry, AuthProbe, FakeTransport, HttpRequest, SendOptions,
};

#[test]
fn probe_auth_classifies_credential_by_status_without_billing() {
    let secret = Secret::new("exa-probe-key-123456").unwrap();

    // 400 INVALID_REQUEST_BODY: auth passed, the empty body failed validation, no search ran.
    let ok = FakeTransport::default();
    ok.push_ok_json(400, r#"{"tag":"INVALID_REQUEST_BODY"}"#);
    assert_eq!(
        probe_auth(&ok, "https://api.exa.ai", &secret).unwrap(),
        AuthProbe::Accepted { status: 400 }
    );
    let req = &ok.recorded_requests()[0];
    assert_eq!(req.method, "POST");
    assert!(req.url.ends_with("/search"));
    assert_eq!(req.body.as_deref(), Some(&b"{}"[..]));
    assert!(req.headers.iter().any(|(k, _)| k == "x-api-key"));

    // 401 INVALID_API_KEY: rejected upstream.
    let rejected = FakeTransport::default();
    rejected.push_ok_json(401, r#"{"tag":"INVALID_API_KEY"}"#);
    assert_eq!(
        probe_auth(&rejected, "https://api.exa.ai", &secret).unwrap(),
        AuthProbe::Rejected { status: 401 }
    );

    // 503 outage: says nothing about the key — must NOT report a valid credential.
    let outage = FakeTransport::default();
    outage.push_ok_json(503, "service unavailable");
    assert_eq!(
        probe_auth(&outage, "https://api.exa.ai", &secret).unwrap(),
        AuthProbe::Inconclusive { status: 503 }
    );
}

#[test]
fn probe_connectivity_ok_on_any_status_fails_only_on_transport_error() {
    // Even an unrouted 404 proves DNS+TLS+reachability.
    let reachable = FakeTransport::default();
    reachable.push_ok_json(404, "not found");
    assert_eq!(
        probe_connectivity(&reachable, "https://api.exa.ai").unwrap(),
        404
    );

    let down = FakeTransport::default();
    down.push_err(CliError::Network(Diag::new("network", "dns failure")));
    assert!(probe_connectivity(&down, "https://api.exa.ai").is_err());
}

#[test]
fn contents_outcome_distinguishes_empty_complete_partial_and_full() {
    assert_eq!(
        contents_outcome(
            &serde_json::json!({
                "results": [],
                "statuses": [{"status": "success"}]
            }),
            1
        ),
        "no_content"
    );
    assert_eq!(
        contents_outcome(
            &serde_json::json!({
                "results": [{"url": "https://ok.test"}],
                "statuses": [{"status": "error"}]
            }),
            1
        ),
        "partial"
    );
    assert_eq!(
        contents_outcome(
            &serde_json::json!({
                "results": [],
                "statuses": [{"status": "error"}]
            }),
            1
        ),
        "partial"
    );
    assert_eq!(
        contents_outcome(
            &serde_json::json!({
                "results": [{"url": "https://ok.test"}],
                "statuses": [{"status": "success"}]
            }),
            1
        ),
        "full"
    );
    assert_eq!(
        contents_outcome(
            &serde_json::json!({"results": [{"url": "https://ok.test"}]}),
            1
        ),
        "full",
        "complete result rows do not require optional statuses"
    );
}

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

#[test]
fn streaming_ndjson_shape_from_canned_sse() {
    use exa_agent_cli::output::envelope::{
        event_envelope, response_envelope, EventEnvelopeArgs, ResponseEnvelopeArgs,
    };
    use exa_agent_cli::transport::{
        data_hash, infer_stream_event_type, parse_sse, primary_count, terminal_stream_data,
    };

    let sse = b"id: evt-1\ndata: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\nid: evt-2\ndata: {\"answer\":\"done\",\"citations\":[]}\n\ndata: [DONE]\n\n";
    let frames = parse_sse(sse);
    let mut lines = Vec::new();
    let mut seq = 0u64;
    for frame in &frames {
        for chunk in &frame.data {
            if chunk == "[DONE]" {
                continue;
            }
            seq += 1;
            let event: serde_json::Value = serde_json::from_str(chunk).unwrap();
            lines.push(
                serde_json::to_string(&event_envelope(EventEnvelopeArgs {
                    event_type: infer_stream_event_type(&event),
                    command: "answer",
                    seq,
                    event_id: frame.id.as_deref(),
                    correlation_id: Some("corr-test"),
                    event,
                }))
                .unwrap(),
            );
        }
    }
    let accumulated = terminal_stream_data(&frames);
    lines.push(
        serde_json::to_string(&response_envelope(ResponseEnvelopeArgs {
            command: "answer",
            method: "POST",
            path: "/answer",
            operation: None,
            request_id: "req_test",
            profile: "default",
            correlation_id: Some("corr-test"),
            data: accumulated.clone(),
            count: primary_count(&accumulated),
            data_hash: data_hash(&accumulated),
            retries: 0,
            duration_ms: 0,
            warnings: &[],
        }))
        .unwrap(),
    );

    assert_eq!(lines.len(), 3);
    let first: serde_json::Value = serde_json::from_str(&lines[0]).unwrap();
    assert_eq!(first["schema"], "exa.cli.event.v1");
    assert_eq!(first["type"], "delta");
    assert_eq!(first["seq"], 1);
    assert_eq!(first["eventId"], "evt-1");
    let last: serde_json::Value = serde_json::from_str(&lines[2]).unwrap();
    assert_eq!(last["schema"], "exa.cli.response.v1");
    assert_eq!(
        last["data"],
        serde_json::json!({"answer":"done","citations":[]})
    );
}

#[test]
fn terminal_stream_data_concatenates_openai_delta_chunks() {
    use exa_agent_cli::transport::{parse_sse, terminal_stream_data};

    let frames = parse_sse(
        b"data: {\"choices\":[{\"delta\":{\"content\":\"hel\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\ndata: [DONE]\n\n",
    );
    assert_eq!(
        terminal_stream_data(&frames),
        serde_json::json!({"answer":"hello"})
    );
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
