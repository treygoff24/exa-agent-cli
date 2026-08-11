//! Transport seam tests (Wave 1D): fake transport, retry policy, header refusal.

use exa_agent_cli::auth::{self, CredentialInput, NoopKeyring, Secret};
use exa_agent_cli::cli::GlobalArgs;
use exa_agent_cli::error::{CliError, Diag};
use exa_agent_cli::transport::{
    answer_outcome, build_url, classify_http_status, contents_outcome, execute_raw,
    execute_raw_with_request_id, looks_binary, parse_user_headers, probe_auth, probe_connectivity,
    row_has_usable_text, send_with_retry, AuthProbe, FakeTransport, HttpRequest, PaymentAuth,
    RawAuth, RawExecuteParams, SendOptions,
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
                "results": [{"url": "https://ok.test", "text": "usable"}],
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
        "no_content"
    );
    assert_eq!(
        contents_outcome(
            &serde_json::json!({
                "results": [{"url": "https://ok.test", "text": "usable"}],
                "statuses": [{"status": "success"}]
            }),
            1
        ),
        "full"
    );
    assert_eq!(
        contents_outcome(
            &serde_json::json!({"results": [{"url": "https://ok.test", "text": "usable"}]}),
            1
        ),
        "full",
        "complete usable result rows do not require optional statuses"
    );
}

#[test]
fn content_text_helpers_reject_empty_and_binary_bodies() {
    assert!(!looks_binary(""));
    assert!(!looks_binary(
        "ordinary UTF-8 prose with em dashes — and accents é"
    ));
    assert!(looks_binary("\u{1f}\u{8b}\u{8}\0gzip"));
    assert!(looks_binary("\0\u{1}\u{2}\u{3}printable"));

    assert!(!row_has_usable_text(&serde_json::json!({"text": "   \n"})));
    assert!(!row_has_usable_text(
        &serde_json::json!({"text": "\u{1f}\u{8b}\u{8}\0gzip"})
    ));
    assert!(row_has_usable_text(
        &serde_json::json!({"text": "usable text"})
    ));
    assert!(row_has_usable_text(
        &serde_json::json!({"text": "", "summary": "usable summary"})
    ));
    assert!(row_has_usable_text(
        &serde_json::json!({"text": "\u{1f}\u{8b}\u{8}\0gzip", "summary": "usable summary"})
    ));
}

#[test]
fn contents_outcome_counts_only_usable_rows() {
    assert_eq!(
        contents_outcome(
            &serde_json::json!({
                "results": [{"url": "https://empty.test", "text": ""}],
                "statuses": [{"id": "https://empty.test", "status": "success"}]
            }),
            1
        ),
        "no_content"
    );
    assert_eq!(
        contents_outcome(
            &serde_json::json!({
                "results": [
                    {"url": "https://ok.test", "text": "usable"},
                    {"url": "https://binary.test", "text": "\u{1f}\u{8b}\u{8}\0gzip"}
                ]
            }),
            2
        ),
        "partial"
    );
}

#[test]
fn answer_outcome_rejects_silent_empty_answers() {
    assert_eq!(
        answer_outcome(&serde_json::json!({"answer": "done"})),
        "full"
    );
    assert_eq!(
        answer_outcome(
            &serde_json::json!({"answer": "", "citations": [{"url": "https://a.test"}]})
        ),
        "partial"
    );
    assert_eq!(
        answer_outcome(&serde_json::json!({"answer": "", "citations": []})),
        "no_content"
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
        follow_redirects: true,
        payment_mode: false,
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
        follow_redirects: true,
        payment_mode: false,
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

/// Regression: a 402 used to fall through the `400..=499` arm and surface as `invalid_value` /
/// exit 1 — a *usage* error. Agents read that as "my flags were wrong" and burned three or four
/// more calls re-guessing arguments against an account that could not pay for any of them.
#[test]
fn status_402_is_billing_not_usage() {
    let err = classify_http_status(
        402,
        br#"{"tag":"NO_MORE_CREDITS","message":"You have exceeded your credits limit. Please top up to keep using Exa at dashboard.exa.ai"}"#,
        &[],
    );
    assert!(matches!(err, CliError::Billing(_)));
    assert_eq!(err.diag().code, "insufficient_credits");
    assert_eq!(err.category(), 13);
    assert_eq!(err.category_name(), "billing");
    assert_eq!(err.diag().http_status, Some(402));

    // Not retryable: credits do not come back on a timer the way a 429 does.
    assert!(!err.diag().retryable);

    // The message must say what is actually wrong and what to do about it.
    let message = err.diag().message.to_ascii_lowercase();
    assert!(message.contains("out of credits"), "message: {message}");
    assert!(message.contains("dashboard.exa.ai"), "message: {message}");
    assert!(
        message.contains("retrying") || message.contains("will not help"),
        "message must tell the agent not to retry: {message}"
    );

    // The exact upstream body is preserved for anything that wants to read the tag.
    let details = err.diag().details.as_ref().unwrap();
    assert_eq!(details["upstream"]["tag"], "NO_MORE_CREDITS");
}

/// Exa has been observed tagging credit exhaustion onto 4xx codes other than 402. The status
/// alone must not be the only signal, or the misclassification comes straight back.
#[test]
fn credit_exhaustion_body_is_billing_on_any_4xx() {
    let err = classify_http_status(400, br#"{"tag":"NO_MORE_CREDITS"}"#, &[]);
    assert_eq!(err.diag().code, "insufficient_credits");
    assert_eq!(err.category(), 13);

    let err = classify_http_status(403, b"You have exceeded your credits limit", &[]);
    assert_eq!(err.diag().code, "insufficient_credits");
    assert_eq!(err.category(), 13);
}

#[test]
fn payment_challenge_402_wins_over_credit_body_sniff() {
    let err = send_with_retry(
        &payment_challenge_transport(br#"{"tag":"NO_MORE_CREDITS"}"#),
        &payment_request("pay_sig_challenge_wins"),
        &payment_send_options(),
    )
    .unwrap_err();
    assert_eq!(err.diag().code, "payment_required");
    assert_eq!(err.category(), 2);
}

#[test]
fn credit_body_sniff_ignores_server_errors() {
    let err = classify_http_status(503, br#"{"tag":"NO_MORE_CREDITS"}"#, &[]);
    assert_eq!(err.diag().code, "upstream_error");
    assert_eq!(err.category(), 5);
    assert!(err.diag().retryable);
}

#[test]
fn payment_www_authenticate_parser_handles_quoted_commas() {
    let fake = FakeTransport::default();
    fake.push_response(exa_agent_cli::transport::HttpResponse {
        status: 402,
        headers: vec![(
            "WWW-Authenticate".to_string(),
            r#"Payment realm="exa \"team, alpha\"", max_amount="0.01""#.to_string(),
        )],
        body: br#"{"message":"payment required"}"#.to_vec(),
    });
    let err = send_with_retry(
        &fake,
        &payment_request("pay_sig_quoted_challenge"),
        &payment_send_options(),
    )
    .unwrap_err();
    assert_eq!(err.diag().code, "payment_required");
    assert_eq!(
        err.diag().details.as_ref().unwrap()["payment"]["headers"][0]["name"],
        "WWW-Authenticate"
    );
}

#[test]
fn mixed_www_authenticate_schemes_are_not_payment_challenges() {
    let fake = FakeTransport::default();
    fake.push_response(exa_agent_cli::transport::HttpResponse {
        status: 402,
        headers: vec![(
            "WWW-Authenticate".to_string(),
            r#"Payment realm="exa", Bearer realm="api""#.to_string(),
        )],
        body: br#"{"message":"payment required"}"#.to_vec(),
    });
    let err = send_with_retry(
        &fake,
        &payment_request("pay_sig_mixed_challenge"),
        &payment_send_options(),
    )
    .unwrap_err();
    assert_eq!(err.diag().code, "insufficient_credits");
}

#[test]
fn malformed_www_authenticate_payment_challenges_fail_closed() {
    for value in [
        r#"PaymentBearer realm="api""#,
        r#"Payment realm="unterminated"#,
        r#"Payment realm="dangling\""#,
    ] {
        let fake = FakeTransport::default();
        fake.push_response(exa_agent_cli::transport::HttpResponse {
            status: 402,
            headers: vec![("WWW-Authenticate".to_string(), value.to_string())],
            body: br#"{"message":"payment required"}"#.to_vec(),
        });
        let err = send_with_retry(
            &fake,
            &payment_request("pay_sig_bad_challenge"),
            &payment_send_options(),
        )
        .unwrap_err();
        assert_eq!(err.diag().code, "insufficient_credits", "{value}");
    }
}

/// The sniff must stay narrow — an ordinary bad request that happens to mention credit is still
/// a usage error, and a real auth failure is still an auth failure.
#[test]
fn credit_sniff_does_not_swallow_ordinary_4xx() {
    let err = classify_http_status(
        400,
        br#"{"message":"invalid query: credit card fraud detection"}"#,
        &[],
    );
    assert!(matches!(err, CliError::Usage(_)));
    assert_eq!(err.diag().code, "invalid_value");

    let err = classify_http_status(401, br#"{"tag":"INVALID_API_KEY"}"#, &[]);
    assert!(matches!(err, CliError::Auth(_)));
    assert_eq!(err.diag().code, "reauth_required");
}

/// A 402 must not be retried. `should_retry` is private, so this asserts through the public
/// send path: one canned 402 and one recorded request means no retry happened.
#[test]
fn billing_error_is_not_retried() {
    let fake = FakeTransport::default();
    fake.push_ok_json(402, r#"{"tag":"NO_MORE_CREDITS"}"#);
    fake.push_ok_json(200, r#"{"ok":true}"#);
    let req = HttpRequest {
        method: "GET".into(),
        url: "https://api.exa.ai/search".into(),
        headers: vec![],
        body: None,
    };
    let opts = SendOptions {
        retry: 3,
        retry_after: false,
        idempotency_key: None,
        follow_redirects: true,
        payment_mode: false,
    };
    let err = send_with_retry(&fake, &req, &opts).unwrap_err();
    assert_eq!(err.diag().code, "insufficient_credits");
    assert_eq!(
        fake.recorded_requests().len(),
        1,
        "a dry account must not be retried"
    );
}

/// The wire shape is what agents actually branch on, so pin it: a 402 must reach stderr as a
/// billing envelope with exit 13, not as a usage envelope with exit 1.
#[test]
fn billing_error_envelope_carries_exit_13() {
    let err = classify_http_status(402, br#"{"tag":"NO_MORE_CREDITS"}"#, &[]);
    let envelope = exa_agent_cli::output::envelope::ErrorEnvelope::from_error(&err).to_json();
    assert_eq!(envelope["schema"], "exa.cli.error.v1");
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "insufficient_credits");
    assert_eq!(envelope["error"]["category"], "billing");
    assert_eq!(envelope["error"]["exitCode"], 13);
    assert_eq!(envelope["error"]["retryable"], false);
    assert_eq!(envelope["error"]["httpStatus"], 402);
}

/// `capabilities` is the generated source of truth agents read; the new codes must be published
/// there, and every code the binary can emit must be a declared member of the dictionary.
#[test]
fn billing_codes_are_published_in_capabilities() {
    let caps = exa_agent_cli::output::envelope::capabilities();
    assert_eq!(caps["exitCodes"]["13"]["name"], "billing");
    assert_eq!(caps["errorCodes"]["insufficient_credits"]["exit"], 13);
    assert_eq!(
        caps["errorCodes"]["insufficient_credits"]["category"],
        "billing"
    );
    assert_eq!(
        caps["errorCodes"]["insufficient_credits"]["retryable"],
        false
    );
    // Previously emitted but undeclared — an agent branching on them found nothing published.
    assert!(caps["errorCodes"].get("probe_inconclusive").is_some());
    assert!(caps["errorCodes"].get("invalid_field_type").is_some());
}

/// Exa reports some failed crawls as `status: "error"` with an empty `error: {}`. Without an
/// explicit label the row carries a crawl failure and no reason whatsoever, which reads to a
/// caller as a silent dead end rather than as upstream declining to say why.
#[test]
fn contents_diagnostics_label_empty_upstream_error_objects() {
    let data = serde_json::json!({
        "statuses": [
            { "id": "https://cato.org/a", "status": "error", "error": {} },
            { "id": "https://cato.org/b", "status": "error",
              "error": { "tag": "CRAWL_NOT_FOUND", "httpStatusCode": 404 } },
        ],
        "results": [],
    });
    let requested = vec![
        "https://cato.org/a".to_string(),
        "https://cato.org/b".to_string(),
    ];
    let diagnostics = exa_agent_cli::transport::contents_diagnostics(&data, &requested);

    let empty = &diagnostics[0];
    assert_eq!(empty["crawl_status"], "error");
    assert_eq!(empty["content_status"], "crawl_error");
    assert_eq!(empty["error_reason"], "upstream_reason_unavailable");
    // Nothing is invented into the exact-upstream fields.
    assert!(empty.get("error_tag").is_none());
    assert!(empty.get("http_status").is_none());

    // A row that *does* carry a reason keeps it verbatim and gets no synthetic label.
    let reported = &diagnostics[1];
    assert_eq!(reported["error_tag"], "CRAWL_NOT_FOUND");
    assert_eq!(reported["http_status"], 404);
    assert!(reported.get("error_reason").is_none());
}

/// The billing-free auth probe is the only credit preflight Exa's API makes possible — there is
/// no balance endpoint — so it has to distinguish "key is bad" from "account is dry".
#[test]
fn probe_auth_reports_out_of_credits_separately_from_rejection() {
    let secret = Secret::new("exa-probe-key-123456").unwrap();

    let dry = FakeTransport::default();
    dry.push_ok_json(402, r#"{"tag":"NO_MORE_CREDITS"}"#);
    assert_eq!(
        probe_auth(&dry, "https://api.exa.ai", &secret).unwrap(),
        AuthProbe::OutOfCredits { status: 402 }
    );

    // A dry account answering some other 4xx with the credits tag is still out of credits,
    // not a valid-and-healthy key.
    let tagged = FakeTransport::default();
    tagged.push_ok_json(400, r#"{"tag":"NO_MORE_CREDITS"}"#);
    assert_eq!(
        probe_auth(&tagged, "https://api.exa.ai", &secret).unwrap(),
        AuthProbe::OutOfCredits { status: 400 }
    );

    // The ordinary healthy-key probe response is unchanged.
    let healthy = FakeTransport::default();
    healthy.push_ok_json(400, r#"{"tag":"INVALID_REQUEST_BODY"}"#);
    assert_eq!(
        probe_auth(&healthy, "https://api.exa.ai", &secret).unwrap(),
        AuthProbe::Accepted { status: 400 }
    );
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
        follow_redirects: true,
        payment_mode: false,
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
        follow_redirects: true,
        payment_mode: false,
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

#[test]
fn successful_payment_trace_scrubs_echoed_payment_secret_values() {
    let fake = FakeTransport::default();
    let secret_value = "pay_sig_success_echo";
    fake.push_response(exa_agent_cli::transport::HttpResponse {
        status: 200,
        headers: vec![
            ("content-type".to_string(), "application/json".to_string()),
            ("x-debug-echo".to_string(), secret_value.to_string()),
            (format!("x-{secret_value}-echo"), "header-name-secret".to_string()),
            (
                format!("x-{}-echo", exa_agent_cli::redaction::REDACTED),
                "collision-preserved".to_string(),
            ),
        ],
        body: format!(
            r#"{{"{secret_value}":"key echo","nonce":"{secret_value}","nested":{{"copy":"{secret_value}"}}}}"#
        )
        .into_bytes(),
    });
    let trace_path = std::env::temp_dir().join(format!(
        "exa-agent-payment-success-trace-{}-{}.jsonl",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let cli = parse_globals(&["--trace", trace_path.to_str().unwrap()]);
    let signature = Secret::new(secret_value).unwrap();
    let query = vec![format!("debug={secret_value}")];

    let result = execute_raw_with_request_id(
        &fake,
        RawExecuteParams {
            method: "POST",
            path: "/search",
            query_raw: &query,
            body: serde_json::json!({"query":"hi"}),
            globals: &cli,
            auth: RawAuth::Payment(PaymentAuth::X402 {
                signature: &signature,
            }),
            request_id: "req_payment_success_trace".to_string(),
        },
    )
    .unwrap();
    assert_eq!(result.response.status, 200);
    assert!(
        !String::from_utf8_lossy(&result.response.body).contains(secret_value),
        "{:?}",
        result.response
    );
    assert!(
        result
            .response
            .headers
            .iter()
            .all(|(_, value)| !value.contains(secret_value)),
        "{:?}",
        result.response.headers
    );

    let trace = std::fs::read_to_string(&trace_path).unwrap();
    let _ = std::fs::remove_file(trace_path);
    assert!(!trace.contains(secret_value), "{trace}");
    assert!(
        trace.contains(exa_agent_cli::redaction::REDACTED),
        "{trace}"
    );
    let record: serde_json::Value = serde_json::from_str(trace.lines().next().unwrap()).unwrap();
    assert_eq!(
        record["url"],
        format!(
            "https://api.exa.ai/search?debug={}",
            exa_agent_cli::redaction::REDACTED
        )
    );
    let headers = record["outcome"]["responseHeaders"].as_object().unwrap();
    let redacted_name = format!("x-{}-echo", exa_agent_cli::redaction::REDACTED);
    assert_eq!(headers[&redacted_name], "header-name-secret");
    assert_eq!(
        headers[&format!("{redacted_name}#2")],
        "collision-preserved"
    );
}

#[test]
fn payment_trace_url_redacts_percent_encoded_x402_and_mpp_secrets() {
    for (auth, secret_value, encoded) in [
        ("x402", "pay+sig/raw=", "pay%2Bsig%2Fraw%3D"),
        (
            "mpp",
            "Payment pay+mpp/raw=",
            "Payment%20pay%2Bmpp%2Fraw%3D",
        ),
    ] {
        let fake = FakeTransport::default();
        fake.push_response(exa_agent_cli::transport::HttpResponse {
            status: 200,
            headers: vec![("content-type".to_string(), "application/json".to_string())],
            body: br#"{"ok":true,"ordinary":"keep"}"#.to_vec(),
        });
        let trace_path = std::env::temp_dir().join(format!(
            "exa-agent-payment-encoded-url-trace-{auth}-{}-{}.jsonl",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let cli = parse_globals(&["--trace", trace_path.to_str().unwrap()]);
        let secret = Secret::new(secret_value).unwrap();
        let query = vec![format!("debug={secret_value}")];
        let raw_auth = if auth == "x402" {
            RawAuth::Payment(PaymentAuth::X402 { signature: &secret })
        } else {
            RawAuth::Payment(PaymentAuth::Mpp {
                authorization: &secret,
            })
        };

        let result = execute_raw_with_request_id(
            &fake,
            RawExecuteParams {
                method: "POST",
                path: "/search",
                query_raw: &query,
                body: serde_json::json!({"query":"hi"}),
                globals: &cli,
                auth: raw_auth,
                request_id: format!("req_payment_encoded_url_{auth}"),
            },
        )
        .unwrap();
        assert_eq!(result.response.body, br#"{"ok":true,"ordinary":"keep"}"#);

        let trace = std::fs::read_to_string(&trace_path).unwrap();
        let _ = std::fs::remove_file(trace_path);
        assert!(!trace.contains(secret_value), "{trace}");
        assert!(!trace.contains(encoded), "{trace}");
        assert!(
            trace.contains(exa_agent_cli::redaction::REDACTED),
            "{trace}"
        );
        assert!(trace.contains(r#""ordinary":"keep""#), "{trace}");
    }
}

#[test]
fn payment_error_details_scrub_secret_json_keys() {
    let fake = FakeTransport::default();
    let secret_value = "pay_sig_error_key_echo";
    fake.push_response(exa_agent_cli::transport::HttpResponse {
        status: 503,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body: format!(r#"{{"{secret_value}":"top","nested":{{"{secret_value}":"nested"}}}}"#)
            .into_bytes(),
    });
    let cli = parse_globals(&[]);
    let signature = Secret::new(secret_value).unwrap();

    let err = execute_raw_with_request_id(
        &fake,
        RawExecuteParams {
            method: "POST",
            path: "/search",
            query_raw: &[],
            body: serde_json::json!({"query":"hi"}),
            globals: &cli,
            auth: RawAuth::Payment(PaymentAuth::X402 {
                signature: &signature,
            }),
            request_id: "req_payment_error_key".to_string(),
        },
    )
    .unwrap_err();

    let details = serde_json::to_string(err.diag().details.as_ref().unwrap()).unwrap();
    assert!(!details.contains(secret_value), "{details}");
    assert!(
        details.contains(exa_agent_cli::redaction::REDACTED),
        "{details}"
    );
}

#[test]
fn successful_payment_raw_response_scrubs_secret_and_preserves_other_bytes() {
    let fake = FakeTransport::default();
    let secret_value = "pay_sig_raw_echo";
    fake.push_response(exa_agent_cli::transport::HttpResponse {
        status: 200,
        headers: vec![
            (
                "x-debug-echo".to_string(),
                format!("prefix-{secret_value}-suffix"),
            ),
            ("x-safe".to_string(), "keep-me".to_string()),
        ],
        body: [
            b"before ".as_slice(),
            &[0xff, 0x00],
            secret_value.as_bytes(),
            b" after".as_slice(),
        ]
        .concat(),
    });
    let cli = parse_globals(&[]);
    let signature = Secret::new(secret_value).unwrap();

    let result = execute_raw_with_request_id(
        &fake,
        RawExecuteParams {
            method: "POST",
            path: "/search",
            query_raw: &[],
            body: serde_json::json!({"query":"hi"}),
            globals: &cli,
            auth: RawAuth::Payment(PaymentAuth::X402 {
                signature: &signature,
            }),
            request_id: "req_payment_raw_response".to_string(),
        },
    )
    .unwrap();

    let expected = [
        b"before ".as_slice(),
        &[0xff, 0x00],
        exa_agent_cli::redaction::REDACTED.as_bytes(),
        b" after".as_slice(),
    ]
    .concat();
    assert_eq!(result.response.body, expected);
    assert!(result
        .response
        .headers
        .iter()
        .any(|(name, value)| name == "x-debug-echo"
            && value == &format!("prefix-{}-suffix", exa_agent_cli::redaction::REDACTED)));
    assert!(result
        .response
        .headers
        .iter()
        .any(|(name, value)| name == "x-safe" && value == "keep-me"));
}

#[test]
fn payment_json_error_preview_redacts_secret_across_truncation_boundary() {
    let secret_value = "JSON_LEFT_SECRET_MID_RIGHT_CANARY";
    let prefix_chars = 4096 - 1 - "JSON_LEFT_SECRET".len();
    let upstream_string = format!("{}{secret_value} after", "J".repeat(prefix_chars));
    let body = serde_json::to_vec(&upstream_string).unwrap();
    let fake = FakeTransport::default();
    fake.push_response(exa_agent_cli::transport::HttpResponse {
        status: 400,
        headers: vec![("content-type".to_string(), "application/json".to_string())],
        body,
    });

    let err = send_with_retry(
        &fake,
        &payment_request(secret_value),
        &payment_no_retry_options(),
    )
    .unwrap_err();

    let details = err.diag().details.as_ref().unwrap();
    assert_eq!(details["upstreamTruncated"], true);
    let preview = details["upstreamPreview"].as_str().unwrap();
    assert!(preview.starts_with('"'));
    assert!(
        preview.contains(exa_agent_cli::redaction::REDACTED),
        "{preview}"
    );
    assert!(preview.len() <= 4096);
    assert_no_secret_fragments(
        &serde_json::to_string(details).unwrap(),
        secret_value,
        &["JSON_LEFT_SECRET", "SECRET_MID_RIGHT", "RIGHT_CANARY"],
    );
}

#[test]
fn payment_non_json_error_preview_redacts_secret_across_truncation_boundary() {
    let secret_value = "NONJSON_LEFT_SECRET_MID_RIGHT_CANARY";
    let prefix_chars = 200 - "NONJSON_LEFT".len();
    let body = format!("{}{secret_value} after", "N".repeat(prefix_chars)).into_bytes();
    let fake = FakeTransport::default();
    fake.push_response(exa_agent_cli::transport::HttpResponse {
        status: 404,
        headers: vec![("content-type".to_string(), "text/html".to_string())],
        body,
    });

    let err = send_with_retry(
        &fake,
        &payment_request(secret_value),
        &payment_no_retry_options(),
    )
    .unwrap_err();

    let details = err.diag().details.as_ref().unwrap();
    let preview = details["bodyPreview"].as_str().unwrap();
    assert!(preview.starts_with("NNNN"));
    assert!(
        preview.contains(exa_agent_cli::redaction::REDACTED),
        "{preview}"
    );
    assert!(preview.chars().count() <= 200);
    assert_no_secret_fragments(
        &serde_json::to_string(details).unwrap(),
        secret_value,
        &["NONJSON_LEFT", "SECRET_MID_RIGHT", "RIGHT_CANARY"],
    );
}

fn payment_challenge_transport(body: &[u8]) -> FakeTransport {
    let fake = FakeTransport::default();
    fake.push_response(exa_agent_cli::transport::HttpResponse {
        status: 402,
        headers: vec![("PAYMENT-REQUIRED".to_string(), "price=0.01".to_string())],
        body: body.to_vec(),
    });
    fake
}

fn payment_request(secret: &str) -> HttpRequest {
    HttpRequest {
        method: "POST".into(),
        url: "https://api.exa.ai/search".into(),
        headers: vec![("PAYMENT-SIGNATURE".into(), secret.into())],
        body: Some(br#"{"query":"hi"}"#.to_vec()),
    }
}

fn payment_send_options() -> SendOptions {
    SendOptions {
        retry: 2,
        retry_after: false,
        idempotency_key: None,
        follow_redirects: false,
        payment_mode: true,
    }
}

fn payment_no_retry_options() -> SendOptions {
    SendOptions {
        retry: 0,
        retry_after: false,
        idempotency_key: None,
        follow_redirects: false,
        payment_mode: true,
    }
}

fn assert_no_secret_fragments(rendered: &str, secret: &str, fragments: &[&str]) {
    assert!(!rendered.contains(secret), "{rendered}");
    for fragment in fragments {
        assert!(
            !rendered.contains(fragment),
            "{fragment} leaked in {rendered}"
        );
    }
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
