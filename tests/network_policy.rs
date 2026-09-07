//! Receive-size policy, connect-timeout enforcement, response compression, and bounded
//! `contents --jobs` concurrency. Every request in this file goes to a loopback fixture.

mod support;

use std::time::{Duration, Instant};

use support::{
    http_response, isolated_command, loopback_command, scratch_dir, stderr_json, stdout_lines,
    Reply, TestServer,
};

const API_KEY: &str = "test-key-abcdef12";

/// A gzip stream of a small `/search` response, produced once and frozen here so the test needs
/// no compression dependency of its own.
const GZIP_SEARCH_BODY: &[u8] = &[
    0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0xff, 0x25, 0x8c, 0x41, 0x0a, 0x80, 0x20,
    0x14, 0x44, 0xaf, 0x12, 0xb3, 0xce, 0xa4, 0xad, 0x37, 0xe8, 0x0c, 0xd1, 0x22, 0xf0, 0x63, 0x82,
    0x81, 0xe9, 0x17, 0x42, 0xf1, 0xee, 0xfd, 0x6a, 0x37, 0x6f, 0x66, 0x78, 0x0d, 0x89, 0x72, 0x09,
    0x9c, 0x61, 0xd6, 0x06, 0x6f, 0x61, 0xe0, 0xaa, 0x9a, 0x31, 0x82, 0x3d, 0x07, 0xfa, 0xd0, 0xc7,
    0x48, 0x76, 0xf8, 0x7f, 0x32, 0x94, 0x14, 0xa4, 0x3e, 0x98, 0x63, 0x36, 0x5a, 0xd3, 0xbd, 0x9f,
    0x31, 0xd0, 0xc4, 0x94, 0x59, 0xbb, 0x8a, 0xbe, 0x8d, 0xa2, 0xbc, 0x8a, 0xe0, 0xf2, 0xda, 0x24,
    0xab, 0x57, 0x81, 0xfe, 0x00, 0xc5, 0xc2, 0xec, 0x18, 0x6b, 0x00, 0x00, 0x00,
];

fn oversized_search_body(min_bytes: usize) -> String {
    let filler = "x".repeat(min_bytes);
    format!(r#"{{"requestId":"req-big","results":[{{"id":"1","text":"{filler}"}}]}}"#)
}

/// Oversize success responses are unusable, not retryable transport failures.
#[test]
fn oversized_response_is_unusable_and_not_retryable() {
    let home = scratch_dir("recv-cap-home");
    let server = TestServer::start(vec![Reply::json(&oversized_search_body(4096))]);
    let output = loopback_command(&home)
        .args([
            "search",
            "big",
            "--api-key",
            API_KEY,
            "--base-url",
            &server.base_url,
            "--max-response-bytes",
            "1024",
            "--retry",
            "3",
            "--compact",
        ])
        .output()
        .expect("run search against the oversized fixture");
    let stats = server.finish();

    assert_eq!(
        output.status.code(),
        Some(5),
        "receive cap must report an unusable response"
    );
    let error = stderr_json(&output);
    assert_eq!(error["error"]["code"], "response_too_large");
    assert_eq!(
        error["error"]["retryable"],
        serde_json::Value::Bool(false),
        "an oversized response is not worth retrying: {error}"
    );
    assert_eq!(error["error"]["details"]["maxResponseBytes"], 1024);
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("max-response-bytes"),
        "the message must name the knob: {error}"
    );
    assert_eq!(
        stats.requests.len(),
        1,
        "--retry 3 must not replay an oversize response"
    );
}

/// A body at exactly the ceiling is accepted; the limit is inclusive as documented.
#[test]
fn response_at_exactly_the_ceiling_is_accepted() {
    let home = scratch_dir("recv-cap-exact-home");
    let body = r#"{"requestId":"req-small","results":[]}"#;
    let server = TestServer::start(vec![Reply::json(body)]);
    let output = loopback_command(&home)
        .args([
            "search",
            "small",
            "--api-key",
            API_KEY,
            "--base-url",
            &server.base_url,
            "--max-response-bytes",
            &body.len().to_string(),
            "--compact",
        ])
        .output()
        .expect("run search at the exact ceiling");
    server.finish();
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Zero is rejected rather than silently meaning "unbounded", on the flag and in the config
/// file. The config leg matters on its own: clap's range check never sees a config value, so
/// without the runtime guard a `max_response_bytes = 0` in TOML would disable the ceiling.
#[test]
fn zero_max_response_bytes_is_refused_from_flag_and_config() {
    let home = scratch_dir("recv-cap-zero-home");
    let flag = isolated_command(&home)
        .args(["search", "q", "--max-response-bytes", "0", "--compact"])
        .output()
        .expect("run search with a zero receive cap");
    assert_eq!(flag.status.code(), Some(1));

    let config = home.join("zero.toml");
    std::fs::write(
        &config,
        "base_url = \"https://api.exa.ai\"\nadmin_base_url = \"https://admin-api.exa.ai/team-management\"\nmax_response_bytes = 0\n",
    )
    .expect("write config with a zero ceiling");
    let server = TestServer::start(vec![Reply::json(r#"{"requestId":"r","results":[]}"#)]);
    let from_config = loopback_command(&home)
        .env("EXA_AGENT_CONFIG", &config)
        .args([
            "search",
            "q",
            "--api-key",
            API_KEY,
            "--base-url",
            &server.base_url,
            "--compact",
        ])
        .output()
        .expect("run search with a zero ceiling in config");
    let stats = server.finish();
    assert_eq!(
        from_config.status.code(),
        Some(1),
        "stdout:\n{}",
        String::from_utf8_lossy(&from_config.stdout)
    );
    assert_eq!(
        stats.requests.len(),
        0,
        "an unusable receive policy must be refused before any billable request"
    );
    let error = stderr_json(&from_config);
    assert_eq!(error["error"]["code"], "invalid_value");
}

/// `--connect-timeout` is parsed before anything else, so `--dry-run` rejects exactly what the
/// live call would reject. It used to be accepted and then discarded, never reaching ureq.
#[test]
fn invalid_connect_timeout_is_rejected_even_in_dry_run() {
    let home = scratch_dir("connect-timeout-home");
    let output = isolated_command(&home)
        .args([
            "search",
            "q",
            "--connect-timeout",
            "banana",
            "--dry-run",
            "--print-request",
            "--compact",
        ])
        .output()
        .expect("run a dry run with a bad connect timeout");
    assert_eq!(
        output.status.code(),
        Some(1),
        "stdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let error = stderr_json(&output);
    assert_eq!(error["error"]["code"], "invalid_value");
    assert!(error["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .contains("connect timeout"));
    assert!(
        output.stdout.is_empty(),
        "a rejected dry run must not print a request preview"
    );
}

/// A valid connect timeout reaches the HTTP stack without shortening the total budget: the
/// fixture answers after a delay longer than the connect timeout, and the call still succeeds.
#[test]
fn connect_timeout_does_not_cap_the_total_request_budget() {
    let home = scratch_dir("connect-timeout-live-home");
    let server = TestServer::start(vec![Reply::json_after(
        Duration::from_millis(600),
        r#"{"requestId":"req-slow","results":[]}"#,
    )]);
    let output = loopback_command(&home)
        .args([
            "search",
            "slow",
            "--api-key",
            API_KEY,
            "--base-url",
            &server.base_url,
            "--connect-timeout",
            "200ms",
            "--timeout",
            "20s",
            "--compact",
        ])
        .output()
        .expect("run search with a short connect timeout");
    server.finish();
    assert!(
        output.status.success(),
        "a 200ms connect budget must not abort a 600ms response\nstderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The client advertises gzip and transparently decodes a gzip-encoded response.
#[test]
fn gzip_responses_are_requested_and_decoded() {
    let home = scratch_dir("gzip-home");
    let server = TestServer::start(vec![Reply::raw(
        Duration::ZERO,
        http_response(200, "application/json", Some("gzip"), GZIP_SEARCH_BODY),
    )]);
    let output = loopback_command(&home)
        .args([
            "search",
            "gz",
            "--api-key",
            API_KEY,
            "--base-url",
            &server.base_url,
            "--compact",
        ])
        .output()
        .expect("run search against the gzip fixture");
    let stats = server.finish();

    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let head = stats.requests.first().expect("one recorded request");
    let advertises_gzip =
        head.lines()
            .filter_map(|line| line.split_once(':'))
            .any(|(name, value)| {
                name.eq_ignore_ascii_case("accept-encoding")
                    && value.to_ascii_lowercase().contains("gzip")
            });
    assert!(advertises_gzip, "request head did not offer gzip:\n{head}");

    let envelope: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout envelope");
    assert_eq!(
        envelope["data"]["results"][0]["id"], "gz-1",
        "the gzip body was not decoded: {envelope}"
    );
}

fn contents_args<'a>(base_url: &'a str, urls: &'a [String]) -> Vec<&'a str> {
    let mut args = vec!["contents"];
    args.extend(urls.iter().map(String::as_str));
    args.extend([
        "--chunk-size",
        "1",
        "--api-key",
        API_KEY,
        "--base-url",
        base_url,
        "--ndjson",
    ]);
    args
}

fn chunk_urls(n: usize) -> Vec<String> {
    (0..n)
        .map(|i| format!("https://example.test/page-{i}"))
        .collect()
}

fn chunk_reply(index: usize, delay: Duration) -> Reply {
    Reply::json_after(
        delay,
        &format!(
            r#"{{"requestId":"req-{index}","results":[{{"id":"chunk-{index}","url":"https://example.test/page-{index}","text":"body {index}"}}],"statuses":[{{"id":"https://example.test/page-{index}","status":"success"}}]}}"#
        ),
    )
}

/// Which `page-N` a request body asked for. Concurrent chunks arrive in kernel-accept order, so
/// a fixture keyed on arrival index would answer chunk 0 with chunk 1's body and make an
/// ordering assertion meaningless.
fn requested_page(request: &str) -> usize {
    let marker = "page-";
    let start = request.rfind(marker).expect("request names a page") + marker.len();
    request[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .expect("page index")
}

/// The default is serial: chunk N+1 is not sent until chunk N has been printed.
#[test]
fn contents_chunks_are_serial_by_default() {
    let home = scratch_dir("jobs-default-home");
    let urls = chunk_urls(4);
    let server = TestServer::start_routed(|_, request| {
        chunk_reply(requested_page(request), Duration::from_millis(150))
    });
    let output = loopback_command(&home)
        .args(contents_args(&server.base_url, &urls))
        .output()
        .expect("run serial contents chunks");
    let stats = server.finish();

    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stats.requests.len(), 4);
    assert_eq!(
        stats.max_concurrent, 1,
        "no --jobs flag must mean one request in flight at a time"
    );
}

/// `--jobs N` overlaps at most N requests and still prints chunks in input order.
#[test]
fn contents_jobs_bounds_concurrency_and_preserves_order() {
    let home = scratch_dir("jobs-parallel-home");
    let urls = chunk_urls(6);
    let server = TestServer::start_routed(|_, request| {
        chunk_reply(requested_page(request), Duration::from_millis(250))
    });
    let started = Instant::now();
    let mut args = contents_args(&server.base_url, &urls);
    args.extend(["--jobs", "3"]);
    let output = loopback_command(&home)
        .args(args)
        .output()
        .expect("run concurrent contents chunks");
    let elapsed = started.elapsed();
    let stats = server.finish();

    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(stats.requests.len(), 6);
    assert!(
        stats.max_concurrent > 1,
        "--jobs 3 did not overlap any requests"
    );
    assert!(
        stats.max_concurrent <= 3,
        "--jobs 3 allowed {} concurrent requests",
        stats.max_concurrent
    );
    // Six 250ms replies at width 3 cannot finish in the 1.5s a serial run would need.
    assert!(
        elapsed < Duration::from_millis(1400),
        "concurrent run took {elapsed:?}, which is no better than serial"
    );

    let ids: Vec<String> = stdout_lines(&output)
        .into_iter()
        .filter_map(|line| {
            line.get("id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .collect();
    let expected: Vec<String> = (0..6).map(|i| format!("chunk-{i}")).collect();
    assert_eq!(
        ids, expected,
        "chunks must be printed in input order regardless of completion order"
    );
}

/// A failing chunk stops the run: the failure is reported, and no later round is admitted.
#[test]
fn contents_jobs_stop_admitting_work_after_a_failure() {
    let home = scratch_dir("jobs-failure-home");
    let urls = chunk_urls(6);
    // Page 1 is in the first round of two, so its failure must prevent round two entirely.
    let server = TestServer::start_routed(|_, request| match requested_page(request) {
        1 => Reply::status(500, r#"{"error":{"message":"chunk boom"}}"#),
        page => chunk_reply(page, Duration::from_millis(50)),
    });
    let mut args = contents_args(&server.base_url, &urls);
    args.extend(["--jobs", "2", "--retry", "0"]);
    let output = loopback_command(&home)
        .args(args)
        .output()
        .expect("run failing concurrent contents chunks");
    let stats = server.finish();

    assert_eq!(
        output.status.code(),
        Some(5),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stats.requests.len(),
        2,
        "a failure in the first round of two must not admit a third request"
    );
    let lines = stdout_lines(&output);
    assert!(
        lines.iter().any(
            |line| line.get("schema").and_then(serde_json::Value::as_str)
                == Some("exa.cli.error.v1")
        ),
        "the failing chunk's diagnostic must still reach stdout: {lines:?}"
    );
}

fn compressed(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(bytes).unwrap();
    gzip.finish().unwrap()
}

#[test]
fn decoded_receive_limit_is_inclusive_for_identity_and_gzip() {
    for gzip in [false, true] {
        for size in [1023, 1024, 1025, 1024 * 1024] {
            let home = scratch_dir("decoded-bound");
            let body = format!("\"{}\"", "x".repeat(size - 2));
            let bytes = if gzip {
                compressed(body.as_bytes())
            } else {
                body.into_bytes()
            };
            let server = TestServer::start(vec![Reply::raw(
                Duration::ZERO,
                http_response(200, "application/json", gzip.then_some("gzip"), &bytes),
            )]);
            let output = loopback_command(&home)
                .args([
                    "search",
                    "q",
                    "--api-key",
                    API_KEY,
                    "--base-url",
                    &server.base_url,
                    "--max-response-bytes",
                    "1024",
                    "--retry",
                    "2",
                    "--json",
                ])
                .output()
                .unwrap();
            assert_eq!(server.finish().requests.len(), 1);
            assert_eq!(
                output.status.code(),
                Some(if size <= 1024 { 0 } else { 5 }),
                "gzip={gzip}, size={size}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            if size > 1024 {
                assert_eq!(stderr_json(&output)["error"]["code"], "response_too_large");
                assert_eq!(stderr_json(&output)["error"]["retryable"], false);
            }
        }
    }
}

#[test]
fn oversized_error_bodies_preserve_http_classification() {
    for (status, exit, code) in [
        (402, 13, "insufficient_credits"),
        (401, 2, "reauth_required"),
        (429, 6, "rate_limited"),
        (500, 5, "upstream_error"),
    ] {
        for size in [20, 4096, 128 * 1024] {
            let home = scratch_dir("error-body-bound");
            let server = TestServer::start(vec![Reply::status(status, &"x".repeat(size))]);
            let output = loopback_command(&home)
                .args([
                    "search",
                    "q",
                    "--api-key",
                    API_KEY,
                    "--base-url",
                    &server.base_url,
                    "--max-response-bytes",
                    "1024",
                    "--retry",
                    "0",
                    "--json",
                ])
                .output()
                .unwrap();
            server.finish();
            assert_eq!(output.status.code(), Some(exit));
            assert_eq!(stderr_json(&output)["error"]["code"], code);
        }
    }
}

#[test]
fn oversized_create_keeps_pending_recovery_without_retry() {
    for size in [20, 4096] {
        let home = scratch_dir("create-bound");
        let body = format!(r#"{{"id":"created","padding":"{}"}}"#, "x".repeat(size));
        let server = TestServer::start(vec![Reply::json(&body)]);
        let output = loopback_command(&home)
            .args([
                "websets",
                "create",
                "--query",
                "fixture",
                "--count",
                "1",
                "--api-key",
                API_KEY,
                "--base-url",
                &server.base_url,
                "--max-response-bytes",
                "1024",
                "--retry",
                "3",
                "--json",
            ])
            .output()
            .unwrap();
        assert_eq!(server.finish().requests.len(), 1);
        assert_eq!(output.status.code(), Some(if size > 1024 { 5 } else { 0 }));
        assert_eq!(home.join("pending-runs.jsonl").exists(), size > 1024);
        if size > 1024 {
            let error = stderr_json(&output);
            assert_eq!(error["error"]["code"], "response_too_large");
            assert!(error["error"]["suggestedCommand"]
                .as_str()
                .unwrap()
                .contains("websets list"));
            assert_eq!(error["error"]["retryable"], false);
        }
    }
}

#[test]
fn keyed_oversize_creates_keep_existing_batch_recovery_rules() {
    for batch in [false, true] {
        for large in [false, true] {
            let home = scratch_dir("keyed-create-cap");
            let body = format!(
                r#"{{"id":"created-fixture","padding":"{}"}}"#,
                "x".repeat(if large { 4096 } else { 20 })
            );
            let server = TestServer::start(vec![Reply::json(&body)]);
            let mut args = if batch {
                vec![
                    "batches",
                    "create",
                    "--requests",
                    r#"[{"customId":"row-1","method":"POST","url":"/search","body":{"query":"fixture"}}]"#,
                ]
            } else {
                vec!["websets", "create", "--query", "fixture", "--count", "1"]
            };
            args.extend([
                "--api-key",
                API_KEY,
                "--base-url",
                &server.base_url,
                "--idempotency-key",
                "fixture-stable-key",
                "--retry",
                "3",
                "--max-response-bytes",
                "1024",
                "--json",
            ]);
            let output = loopback_command(&home).args(args).output().unwrap();
            assert_eq!(server.finish().requests.len(), 1);
            assert_eq!(output.status.code(), Some(if large { 5 } else { 0 }));
            assert_eq!(home.join("pending-runs.jsonl").exists(), large && batch);
            if large {
                let error = stderr_json(&output);
                assert_eq!(error["error"]["code"], "response_too_large");
                assert_eq!(error["error"]["retryable"], false);
                if batch {
                    assert!(error["error"]["suggestedCommand"]
                        .as_str()
                        .unwrap()
                        .contains("batches list"));
                }
            }
        }
    }
}

#[test]
fn stream_receive_limit_covers_decoded_sse_and_json_fallback() {
    for gzip in [false, true] {
        for mode in ["--raw", "--ndjson", "--json"] {
            for large in [false, true] {
                for sse in [false, true] {
                    let home = scratch_dir("stream-bound");
                    let text = "x".repeat(if large { 4096 } else { 20 });
                    let prefix = "id: evt-1\nevent: message\ndata: {\"answer\":\"first\"}\n\n";
                    let body = if sse {
                        format!("{prefix}id: evt-2\nevent: message\ndata: {{\"answer\":\"{text}\"}}\n\n")
                    } else {
                        format!(r#"{{"answer":"{text}"}}"#)
                    };
                    let bytes = if gzip {
                        compressed(body.as_bytes())
                    } else {
                        body.into_bytes()
                    };
                    let server = TestServer::start(vec![Reply::raw(
                        Duration::ZERO,
                        http_response(
                            200,
                            if sse {
                                "text/event-stream"
                            } else {
                                "application/json"
                            },
                            gzip.then_some("gzip"),
                            &bytes,
                        ),
                    )]);
                    let output = loopback_command(&home)
                        .args([
                            "answer",
                            "q",
                            "--stream",
                            mode,
                            "--api-key",
                            API_KEY,
                            "--base-url",
                            &server.base_url,
                            "--max-response-bytes",
                            "1024",
                            "--retry",
                            "2",
                        ])
                        .output()
                        .unwrap();
                    assert_eq!(server.finish().requests.len(), 1);
                    assert_eq!(
                        output.status.code(),
                        Some(if large { 5 } else { 0 }),
                        "gzip={gzip} mode={mode} sse={sse}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    );
                    if large {
                        let error = stderr_json(&output);
                        assert_eq!(error["error"]["code"], "response_too_large");
                        assert_eq!(error["error"]["retryable"], false);
                        if sse {
                            assert_eq!(error["error"]["details"]["lastEventId"], "evt-1");
                            if mode != "--json" {
                                assert!(String::from_utf8_lossy(&output.stdout).contains("first"));
                            }
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn every_admitted_chunk_survives_first_middle_last_and_partial_failures() {
    for fail_page in 0..3 {
        for partial in [false, true] {
            let home = scratch_dir("round-drain");
            let server = TestServer::start_routed(move |_, request| {
                let page = requested_page(request);
                if page != fail_page {
                    return chunk_reply(page, Duration::from_millis(20));
                }
                if partial {
                    Reply::json(&format!(
                        r#"{{"results":[],"statuses":[{{"id":"https://example.test/page-{page}","status":"error","error":{{"tag":"CRAWL_ERROR"}}}}]}}"#
                    ))
                } else {
                    Reply::status(500, r#"{"error":"failed"}"#)
                }
            });
            let urls = chunk_urls(6);
            let mut args = contents_args(&server.base_url, &urls);
            args.extend(["--jobs", "3", "--retry", "0"]);
            let output = loopback_command(&home).args(args).output().unwrap();
            assert_eq!(server.finish().requests.len(), 3);
            assert_eq!(output.status.code(), Some(if partial { 10 } else { 5 }));
            let ids: Vec<_> = stdout_lines(&output)
                .iter()
                .filter_map(|v| v["id"].as_str().map(str::to_owned))
                .collect();
            assert_eq!(
                ids,
                (0..3)
                    .filter(|p| *p != fail_page)
                    .map(|p| format!("chunk-{p}"))
                    .collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn chunk_output_keeps_every_success_in_serial_and_parallel_runs() {
    for jobs in [1, 3] {
        for failed in [false, true] {
            for mode in ["--json", "--ndjson"] {
                let home = scratch_dir("chunk-output");
                let path = home.join("chunks.json");
                std::fs::write(&path, "old-data").unwrap();
                let server = TestServer::start_routed(move |_, request| {
                    let page = requested_page(request);
                    if failed && page == 1 {
                        Reply::status(500, r#"{"error":"failed"}"#)
                    } else {
                        chunk_reply(page, Duration::ZERO)
                    }
                });
                let urls = chunk_urls(3);
                let mut args = vec!["contents"];
                args.extend(urls.iter().map(String::as_str));
                let jobs_string = jobs.to_string();
                args.extend([
                    "--chunk-size",
                    "1",
                    "--jobs",
                    &jobs_string,
                    "--retry",
                    "0",
                    "--api-key",
                    API_KEY,
                    "--base-url",
                    &server.base_url,
                    "--output",
                    path.to_str().unwrap(),
                    mode,
                ]);
                let output = loopback_command(&home).args(args).output().unwrap();
                let stats = server.finish();
                assert_eq!(output.status.code(), Some(if failed { 5 } else { 0 }));
                assert_eq!(
                    stats.requests.len(),
                    if failed && jobs == 1 { 2 } else { 3 }
                );
                let data = std::fs::read_to_string(&path).unwrap();
                let docs: Vec<serde_json::Value> = serde_json::Deserializer::from_str(&data)
                    .into_iter()
                    .map(Result::unwrap)
                    .collect();
                let ids: Vec<_> = docs
                    .iter()
                    .flat_map(|doc| {
                        if mode == "--ndjson" {
                            doc["id"]
                                .as_str()
                                .map(str::to_owned)
                                .into_iter()
                                .collect::<Vec<_>>()
                        } else {
                            doc["data"]["results"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|r| r["id"].as_str().unwrap().to_owned())
                                .collect()
                        }
                    })
                    .collect();
                let expected: Vec<_> = (0..3)
                    .filter(|p| !failed || (*p != 1 && (jobs > 1 || *p == 0)))
                    .map(|p| format!("chunk-{p}"))
                    .collect();
                assert_eq!(ids, expected);
                assert_eq!(
                    stdout_lines(&output)
                        .iter()
                        .filter(|v| v.get("dataPath").is_some())
                        .count(),
                    1
                );
            }
        }
    }
}

#[cfg(unix)]
#[test]
fn chunk_output_preserves_destination_modes_and_symlinks() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    for dangling in [false, true] {
        let home = scratch_dir("chunk-symlink");
        let target = home.join("target.json");
        if !dangling {
            std::fs::write(&target, "original").unwrap();
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let path = home.join("link.json");
        symlink("target.json", &path).unwrap();
        let server =
            TestServer::start_routed(|_, req| chunk_reply(requested_page(req), Duration::ZERO));
        let urls = chunk_urls(3);
        let mut args = contents_args(&server.base_url, &urls);
        args.extend(["--jobs", "3", "--output", path.to_str().unwrap()]);
        let output = loopback_command(&home).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        server.finish();
        assert!(path.is_symlink());
        let data = std::fs::read_to_string(&target).unwrap();
        for i in 0..3 {
            assert!(data.contains(&format!("chunk-{i}")));
        }
        if !dangling {
            assert_eq!(
                std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let server = TestServer::start(vec![Reply::status(500, r#"{"error":"failed"}"#)]);
        let urls = chunk_urls(1);
        let mut args = contents_args(&server.base_url, &urls);
        args.extend(["--retry", "0", "--output", path.to_str().unwrap()]);
        let output = loopback_command(&home).args(args).output().unwrap();
        server.finish();
        assert_eq!(output.status.code(), Some(5));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), data);
        assert!(path.is_symlink());
    }
}

#[cfg(unix)]
#[test]
fn late_chunk_write_failure_preserves_file_prefix_and_inline_remaining_results() {
    for dangling in [false, true] {
        let home = scratch_dir("late-output-error");
        let path = home.join("chunks.json");
        if dangling {
            std::os::unix::fs::symlink("new-target.json", &path).unwrap();
        }
        let server = TestServer::start_routed(|_, request| {
            let page = requested_page(request);
            let text = if page == 1 {
                "x".repeat(10000)
            } else {
                "small".into()
            };
            Reply::json(&format!(
                r#"{{"results":[{{"id":"chunk-{page}","url":"https://example.test/page-{page}","text":"{text}"}}]}}"#
            ))
        });
        let urls = chunk_urls(6);
        let mut args = contents_args(&server.base_url, &urls);
        args.extend([
            "--jobs",
            "3",
            "--retry",
            "0",
            "--output",
            path.to_str().unwrap(),
        ]);
        // POSIX sh ulimit -f is in 512-byte units: first chunk fits, second does not.
        // Ignoring XFSZ turns the actual filesystem write into EFBIG, not process death.
        let output = support::isolated_program(&home, "sh")
            .env_remove("EXA_AGENT_NO_NETWORK")
            .args([
                "-c",
                "ulimit -f 8; trap '' XFSZ; exec \"$@\"",
                "sh",
                env!("CARGO_BIN_EXE_exa-agent"),
            ])
            .args(args)
            .output()
            .unwrap();
        assert_ne!(output.status.code(), Some(0));
        assert!(
            output.status.code().is_some(),
            "fixture must report a write error, not die by signal"
        );
        assert_eq!(server.finish().requests.len(), 3);
        let data = std::fs::read_to_string(&path).unwrap();
        let docs: Vec<serde_json::Value> = serde_json::Deserializer::from_str(&data)
            .into_iter()
            .map(Result::unwrap)
            .collect();
        assert!(docs.iter().any(|v| v["id"] == "chunk-0"));
        assert!(!data.contains("chunk-1"));
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("chunk-1"));
        assert!(stdout.contains("chunk-2"));
        assert!(stdout.contains("output_write_failed"));
        if dangling {
            assert!(path.is_symlink());
        }
    }
}

#[test]
fn chunked_stream_body_is_refused_before_requests_or_output() {
    for jobs in ["1", "3"] {
        for streaming in [false, true] {
            let home = scratch_dir("chunk-stream-refusal");
            let path = home.join("output.json");
            std::fs::write(&path, "original destination").unwrap();
            let server =
                TestServer::start_routed(|_, req| chunk_reply(requested_page(req), Duration::ZERO));
            let urls = chunk_urls(3);
            let mut args = contents_args(&server.base_url, &urls);
            args.extend([
                "--jobs",
                jobs,
                "--body",
                if streaming {
                    r#"{"stream":true}"#
                } else {
                    r#"{"stream":false}"#
                },
                "--output",
                path.to_str().unwrap(),
            ]);
            let output = loopback_command(&home).args(args).output().unwrap();
            let stats = server.finish();
            if streaming {
                assert_eq!(output.status.code(), Some(1));
                assert_eq!(
                    stderr_json(&output)["error"]["code"],
                    "invalid_flag_combination"
                );
                assert!(stderr_json(&output)["error"]["message"]
                    .as_str()
                    .unwrap()
                    .contains("stream"));
                assert_eq!(stats.requests.len(), 0);
                assert_eq!(
                    std::fs::read_to_string(&path).unwrap(),
                    "original destination"
                );
                assert_eq!(std::fs::read_dir(&home).unwrap().count(), 1);
            } else {
                assert!(output.status.success());
                assert_eq!(stats.requests.len(), 3);
            }
        }
    }
}

#[test]
fn explicit_jobs_requires_chunk_size_but_default_contents_does_not() {
    let home = scratch_dir("jobs-requires-chunks");
    for args in [
        vec![
            "contents",
            "https://example.test/page",
            "--dry-run",
            "--json",
        ],
        vec![
            "contents",
            "https://example.test/page",
            "--jobs",
            "2",
            "--chunk-size",
            "1",
            "--dry-run",
            "--json",
        ],
    ] {
        assert!(isolated_command(&home)
            .args(args)
            .output()
            .unwrap()
            .status
            .success());
    }
    for jobs in ["1", "3"] {
        let output = isolated_command(&home)
            .args([
                "contents",
                "https://example.test/page",
                "--jobs",
                jobs,
                "--dry-run",
                "--json",
            ])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8_lossy(&output.stderr).contains("--chunk-size"));
        assert!(output.stdout.is_empty());
    }
}

#[cfg(unix)]
#[test]
fn chunk_output_canonicalization_errors_preserve_existing_data() {
    use std::os::unix::fs::PermissionsExt;
    for denied in [false, true] {
        let home = scratch_dir("output-canonicalization");
        let parent = home.join("private");
        std::fs::create_dir(&parent).unwrap();
        let path = parent.join("output.json");
        let original = "original content that must remain untouched";
        std::fs::write(&path, original).unwrap();
        if denied {
            std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o000)).unwrap();
        }
        let server =
            TestServer::start_routed(|_, req| chunk_reply(requested_page(req), Duration::ZERO));
        let urls = chunk_urls(3);
        let mut args = contents_args(&server.base_url, &urls);
        args.extend(["--jobs", "3", "--output", path.to_str().unwrap()]);
        let output = loopback_command(&home).args(args).output().unwrap();
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700)).unwrap();
        let stats = server.finish();
        if denied {
            assert!(!output.status.success());
            assert_eq!(stats.requests.len(), 0);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        } else {
            assert!(output.status.success());
            assert_eq!(stats.requests.len(), 3);
            assert!(!std::fs::read_to_string(&path).unwrap().contains(original));
        }
        assert_eq!(std::fs::read_dir(&parent).unwrap().count(), 1);
    }
}
