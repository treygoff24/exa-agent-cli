use serde_json::Value;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const BATCH_BETA: &str = "batches-2026-06-06";
const AGENT_STOP_BETA: &str = "agent-max-effort-2026-07-27";
const VALID_REQUESTS: &str =
    r#"[{"customId":"row-1","method":"POST","url":"/search","body":{"query":"AI"}}]"#;

fn run(args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_exa-agent"));
    command
        .args(args)
        .env_remove("EXA_AGENT_NO_NETWORK")
        .env_remove("EXA_API_KEY")
        .env_remove("EXA_SERVICE_KEY")
        .env_remove("EXA_PROFILE")
        .env_remove("EXA_OUTPUT")
        .env("EXA_AGENT_CONFIG", temp_path("config").join("config.toml"))
        .env(
            "EXA_AGENT_CREDENTIALS",
            temp_path("credentials").join("credentials.json"),
        );
    command
        .output()
        .unwrap_or_else(|err| panic!("failed to run exa-agent {args:?}: {err}"))
}

fn run_owned(args: &[String]) -> Output {
    run(&args.iter().map(String::as_str).collect::<Vec<_>>())
}

fn stdout_json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout JSON")
}

fn stderr_json(output: &Output) -> Value {
    assert!(output.stdout.is_empty(), "error stdout must be empty");
    serde_json::from_slice(&output.stderr).unwrap_or_else(|err| {
        panic!(
            "stderr was not JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn temp_path(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "exa-agent-batches-{label}-{}-{stamp}",
        std::process::id()
    ))
}

fn read_http_request(stream: &mut std::net::TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => bytes.extend_from_slice(&chunk[..n]),
            Err(err)
                if err.kind() == ErrorKind::WouldBlock || err.kind() == ErrorKind::TimedOut =>
            {
                break;
            }
            Err(err) => panic!("failed to read request: {err}"),
        }
        if request_complete(&bytes) {
            break;
        }
    }
    String::from_utf8(bytes).expect("HTTP request UTF-8")
}

fn request_complete(bytes: &[u8]) -> bool {
    let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let header_end = header_end + 4;
    let headers = String::from_utf8_lossy(&bytes[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    bytes.len() >= header_end + content_length
}

fn local_json_server<F>(validate: F, response: &'static str) -> (String, thread::JoinHandle<()>)
where
    F: FnOnce(String) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept request");
        validate(read_http_request(&mut stream));
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response.len(),
            response
        )
        .unwrap();
        stream.flush().unwrap();
    });
    (format!("http://{address}"), server)
}

fn assert_beta_header(request: &str, expected: &str) {
    let (name, value) = expected.split_once(':').unwrap();
    assert!(
        request.lines().filter_map(|line| line.split_once(':')).any(
            |(actual_name, actual_value)| actual_name.eq_ignore_ascii_case(name)
                && actual_value.trim() == value.trim()
        ),
        "missing exact beta header `{expected}`:\n{request}"
    );
}

#[test]
fn create_preserves_mixed_request_bodies_and_override_precedence() {
    let requests = r#"[
      {"customId":"search-row","method":"POST","url":"/search","body":{"query":"AI","futureSearch":{"keep":true}}},
      {"customId":"agent-row","method":"POST","url":"/agent/runs","body":{"query":"research","futureAgent":7}}
    ]"#;
    let output = run(&[
        "batches",
        "create",
        "--requests",
        requests,
        "--metadata",
        r#"{"layer":"named"}"#,
        "--body",
        r#"{"metadata":{"layer":"body","bodyOnly":"keep"},"futureWrapper":{"keep":true}}"#,
        "--set",
        "metadata.layer=set",
        "--dry-run",
        "--compact",
    ]);
    let value = stdout_json(&output);
    let body = &value["data"]["request"]["body"];
    assert_eq!(body["requests"][0]["body"]["futureSearch"]["keep"], true);
    assert_eq!(body["requests"][1]["body"]["futureAgent"], 7);
    assert_eq!(
        body["metadata"],
        serde_json::json!({"layer":"set","bodyOnly":"keep"})
    );
    assert_eq!(body["futureWrapper"]["keep"], true);

    let body_only = stdout_json(&run(&[
        "batches",
        "create",
        "--body",
        r#"{"requests":[{"customId":"body-only","method":"POST","url":"/search","body":{"query":"body route","future":true}}]}"#,
        "--dry-run",
        "--compact",
    ]));
    assert_eq!(
        body_only["data"]["request"]["body"]["requests"][0]["body"]["future"],
        true
    );
}

#[test]
fn create_json_flags_accept_files() {
    let directory = temp_path("json-files");
    fs::create_dir_all(&directory).unwrap();
    let requests = directory.join("requests.json");
    let metadata = directory.join("metadata.json");
    fs::write(&requests, VALID_REQUESTS).unwrap();
    fs::write(&metadata, r#"{"source":"file"}"#).unwrap();

    let output = run_owned(&[
        "batches".into(),
        "create".into(),
        "--requests".into(),
        format!("@{}", requests.display()),
        "--metadata".into(),
        format!("@{}", metadata.display()),
        "--dry-run".into(),
        "--compact".into(),
    ]);
    let value = stdout_json(&output);
    assert_eq!(
        value["data"]["request"]["body"]["metadata"]["source"],
        "file"
    );
}

#[test]
fn create_rejects_invalid_wrapper_invariants_before_auth() {
    let cases = [
        ("[]", None, "invalid_value"),
        (
            r#"[{"customId":"same","method":"POST","url":"/search","body":{}},{"customId":"same","method":"POST","url":"/agent/runs","body":{}}]"#,
            None,
            "invalid_value",
        ),
        (
            r#"[{"customId":"","method":"POST","url":"/search","body":{}}]"#,
            None,
            "invalid_value",
        ),
        (
            r#"[{"customId":"xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx","method":"POST","url":"/search","body":{}}]"#,
            None,
            "invalid_value",
        ),
        (
            r#"[{"customId":"row","method":"GET","url":"/search","body":{}}]"#,
            None,
            "invalid_value",
        ),
        (
            r#"[{"customId":"row","method":"POST","url":"/contents","body":{}}]"#,
            None,
            "invalid_value",
        ),
        (
            r#"[{"customId":"row","method":"POST","url":"/search","body":{"stream":true}}]"#,
            None,
            "invalid_flag_combination",
        ),
        (
            VALID_REQUESTS,
            Some(r#"{"number":7}"#),
            "invalid_field_type",
        ),
    ];

    for (requests, metadata, code) in cases {
        let mut args = vec!["batches", "create", "--requests", requests];
        if let Some(metadata) = metadata {
            args.extend(["--metadata", metadata]);
        }
        args.push("--compact");
        let output = run(&args);
        assert_eq!(output.status.code(), Some(1), "args={args:?}");
        let error = stderr_json(&output);
        assert_eq!(error["error"]["code"], code, "args={args:?}");
        assert_ne!(error["error"]["code"], "not_authenticated");
    }
}

#[test]
fn typed_previews_auto_merge_required_beta_tokens() {
    let create = stdout_json(&run(&[
        "batches",
        "create",
        "--requests",
        VALID_REQUESTS,
        "--beta",
        "caller-token",
        "--dry-run",
        "--compact",
    ]));
    assert_eq!(
        create["data"]["request"]["headers"],
        serde_json::json!([{"name":"Exa-Beta","value":format!("caller-token,{BATCH_BETA}")}])
    );

    let stop = stdout_json(&run(&[
        "agent",
        "runs",
        "stop",
        "agent_run_abc",
        "--dry-run",
        "--compact",
    ]));
    assert_eq!(
        stop["data"]["request"]["headers"],
        serde_json::json!([{"name":"Exa-Beta","value":AGENT_STOP_BETA}])
    );
}

#[test]
fn live_create_sends_merged_beta_and_adds_poll_action() {
    let response = r#"{"id":"batch_abc","status":"in_progress"}"#;
    let (base_url, server) = local_json_server(
        |request| {
            assert!(request.starts_with("POST /batches "), "{request}");
            assert_beta_header(&request, &format!("Exa-Beta: caller-token,{BATCH_BETA}"));
            assert!(request.contains(r#""customId":"row-1""#), "{request}");
        },
        response,
    );
    let output = run_owned(&[
        "batches".into(),
        "create".into(),
        "--requests".into(),
        VALID_REQUESTS.into(),
        "--beta".into(),
        "caller-token".into(),
        "--api-key".into(),
        "test-key-abcdef12".into(),
        "--base-url".into(),
        base_url,
        "--compact".into(),
    ]);
    server.join().expect("batch create server");
    let value = stdout_json(&output);
    let action = value["nextActions"][0]["command"].as_str().unwrap();
    assert!(
        action.starts_with("'exa-agent' 'batches' 'get' "),
        "{action}"
    );
    assert!(action.ends_with(" '--' 'batch_abc'"), "{action}");
    assert!(action.contains("--beta=caller-token,batches-2026-06-06"));
    assert!(!action.contains("test-key-abcdef12"));
}

#[test]
fn live_agent_stop_uses_distinct_route_confirmation_and_merged_beta() {
    let (base_url, server) = local_json_server(
        |request| {
            assert!(
                request.starts_with("POST /agent/runs/agent_run_stop/stop "),
                "{request}"
            );
            assert!(!request.starts_with("POST /agent/runs/agent_run_stop/cancel "));
            assert_beta_header(
                &request,
                &format!("Exa-Beta: caller-token,{AGENT_STOP_BETA}"),
            );
        },
        r#"{"id":"agent_run_stop","status":"completed","stopReason":"stopped"}"#,
    );
    let output = run_owned(&[
        "agent".into(),
        "runs".into(),
        "stop".into(),
        "agent_run_stop".into(),
        "--yes".into(),
        "--beta".into(),
        "caller-token".into(),
        "--api-key".into(),
        "test-key-abcdef12".into(),
        "--base-url".into(),
        base_url,
        "--compact".into(),
    ]);
    server.join().expect("agent stop server");
    let value = stdout_json(&output);
    assert_eq!(value["data"]["stopReason"], "stopped");
}

#[test]
fn list_all_preserves_status_and_supports_limit_above_100() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let responses = [
            r#"{"data":[{"id":"batch_1"}],"hasMore":true,"nextCursor":"next"}"#,
            r#"{"data":[{"id":"batch_2"}],"hasMore":false,"nextCursor":null}"#,
        ];
        for (index, response) in responses.into_iter().enumerate() {
            let (mut stream, _) = listener.accept().expect("accept page");
            let request = read_http_request(&mut stream);
            let expected_path = if index == 0 {
                "GET /batches?status=completed&limit=101 "
            } else {
                "GET /batches?status=completed&limit=101&cursor=next "
            };
            assert!(request.starts_with(expected_path), "{request}");
            assert_beta_header(&request, &format!("Exa-Beta: caller-token,{BATCH_BETA}"));
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response.len(),
                response
            )
            .unwrap();
            stream.flush().unwrap();
        }
    });
    let output = run_owned(&[
        "batch".into(),
        "list".into(),
        "--all".into(),
        "--status".into(),
        "completed".into(),
        "--limit".into(),
        "101".into(),
        "--beta".into(),
        "caller-token".into(),
        "--api-key".into(),
        "test-key-abcdef12".into(),
        "--base-url".into(),
        format!("http://{address}"),
        "--compact".into(),
    ]);
    server.join().expect("pagination server");
    let value = stdout_json(&output);
    assert_eq!(value["data"]["data"].as_array().unwrap().len(), 2);
}

#[test]
fn list_rejects_zero_limit_before_auth() {
    let output = run(&["batches", "list", "--limit", "0", "--compact"]);
    assert_eq!(output.status.code(), Some(1));
    let error = stderr_json(&output);
    assert_eq!(error["error"]["code"], "invalid_value");
    assert_eq!(error["error"]["details"]["min"], 1);
}

#[test]
fn completed_get_offers_credential_free_results_download() {
    let results_url = "https://objects.example.test/result.jsonl?signature=short-lived";
    let response = Box::leak(
        serde_json::json!({
            "id": "batch_done",
            "status": "completed",
            "resultsUrl": results_url,
        })
        .to_string()
        .into_boxed_str(),
    );
    let (base_url, server) = local_json_server(
        |request| {
            assert!(request.starts_with("GET /batches/batch_done "), "{request}");
            assert_beta_header(&request, &format!("Exa-Beta: {BATCH_BETA}"));
        },
        response,
    );
    let output = run_owned(&[
        "batches".into(),
        "get".into(),
        "batch_done".into(),
        "--api-key".into(),
        "test-key-abcdef12".into(),
        "--base-url".into(),
        base_url,
        "--compact".into(),
    ]);
    server.join().expect("batch get server");
    let value = stdout_json(&output);
    let action = &value["nextActions"][0];
    assert!(action["description"]
        .as_str()
        .unwrap()
        .contains("short-lived"));
    let command = action["command"].as_str().unwrap();
    assert!(command.contains(results_url));
    assert!(!command.contains("Authorization"));
    assert!(!command.contains("test-key"));
}

#[test]
fn cancellation_deletion_and_stop_require_yes_but_preview_without_it() {
    let cases = [
        (
            vec!["batches", "cancel", "batch_abc"],
            "/batches/batch_abc/cancel",
        ),
        (vec!["batches", "delete", "batch_abc"], "/batches/batch_abc"),
        (
            vec!["agent", "runs", "stop", "agent_run_abc"],
            "/agent/runs/agent_run_abc/stop",
        ),
    ];
    for (args, path) in cases {
        let mut preview_args = args.clone();
        preview_args.extend(["--dry-run", "--compact"]);
        let preview = stdout_json(&run(&preview_args));
        assert_eq!(preview["data"]["request"]["path"], path);

        let mut live_args = args;
        live_args.extend(["--api-key", "test-key-abcdef12", "--compact"]);
        let output = run(&live_args);
        assert_eq!(output.status.code(), Some(9));
        assert_eq!(
            stderr_json(&output)["error"]["code"],
            "confirmation_required"
        );
    }
}

#[test]
fn batch_create_ambiguous_failure_suggests_listing() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept request");
        let _ = read_http_request(&mut stream);
        // Drop after reading the full request without returning an HTTP response.
    });
    let pending_path = temp_path("pending").join("pending-runs.jsonl");
    fs::create_dir_all(pending_path.parent().unwrap()).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_exa-agent"));
    let output = command
        .args([
            "batches",
            "create",
            "--requests",
            VALID_REQUESTS,
            "--api-key",
            "test-key-abcdef12",
            "--base-url",
            &format!("http://{address}"),
            "--compact",
        ])
        .env("EXA_AGENT_PENDING_RUNS", &pending_path)
        .env_remove("EXA_AGENT_NO_NETWORK")
        .output()
        .expect("run ambiguous create");
    server.join().expect("drop server");
    assert!(!output.status.success());
    let error = stderr_json(&output);
    assert_eq!(error["error"]["details"]["pendingRunWritten"], true);
    assert_eq!(
        error["error"]["suggestedCommand"],
        format!("exa-agent '--base-url=http://{address}' '--beta=batches-2026-06-06' batches list --limit 10")
    );
    let record: Value = serde_json::from_str(
        fs::read_to_string(&pending_path)
            .expect("pending record")
            .lines()
            .next()
            .expect("pending line"),
    )
    .unwrap();
    assert_eq!(record["command"], "batches create");
    assert_eq!(
        record["recoveryCommand"],
        error["error"]["suggestedCommand"]
    );
}

#[test]
fn capabilities_include_all_six_new_operations() {
    let value = stdout_json(&run(&["capabilities", "--json"]));
    let commands = value["commands"].as_array().unwrap();
    for (path, operation_id) in [
        ("batches create", "createBatch"),
        ("batches list", "listBatches"),
        ("batches get", "getBatch"),
        ("batches cancel", "cancelBatch"),
        ("batches delete", "deleteBatch"),
        ("agent runs stop", "stopAgentRun"),
    ] {
        let command = commands
            .iter()
            .find(|command| command["path"] == path)
            .unwrap_or_else(|| panic!("missing capability {path}"));
        assert_eq!(command["operationId"], operation_id);
    }
}

#[test]
fn schema_validation_checks_batch_wrapper_not_just_json_field_types() {
    for (body, valid) in [
        (
            serde_json::json!({"requests":serde_json::from_str::<Value>(VALID_REQUESTS).unwrap()}),
            true,
        ),
        (serde_json::json!({"requests":[]}), false),
        (
            serde_json::json!({"requests":[{"customId":"a","method":"GET","url":"/search","body":{}}]}),
            false,
        ),
        (
            serde_json::json!({"requests":[{"customId":"a","method":"POST","url":"/search","body":{"stream":true}}]}),
            false,
        ),
    ] {
        let output = run(&[
            "schema",
            "validate-input",
            "batches create",
            "--body",
            &body.to_string(),
            "--json",
        ]);
        assert_eq!(stdout_json(&output)["valid"], valid);
    }
}

#[test]
fn batch_missing_fields_and_cursor_metadata_are_actionable() {
    let output = run(&[
        "batches",
        "create",
        "--requests",
        r#"[{"customId":"a","url":"/search","body":{}}]"#,
        "--dry-run",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr_json(&output)["error"]["code"],
        "missing_required_argument"
    );
    let output = run(&["capabilities", "batches", "list", "--json"]);
    let value = stdout_json(&output);
    assert_eq!(value["command"]["pagination"]["cursorField"], "cursor");
}

#[test]
fn batch_preset_supplies_required_requests_and_explicit_body_overrides_it() {
    let directory = temp_path("batch-preset");
    fs::create_dir_all(&directory).unwrap();
    let preset_path = directory.join("presets.toml");
    fs::write(
        &preset_path,
        r#"
[presets.demo]
command = "batches create"
[presets.demo.body]
requests = [{customId="preset",method="POST",url="/search",body={query="preset query"}}]
metadata = {origin="preset"}
"#,
    )
    .unwrap();
    for override_body in [None, Some(format!(r#"{{"requests":{VALID_REQUESTS}}}"#))] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_exa-agent"));
        command
            .args([
                "batches",
                "create",
                "--preset",
                "demo",
                "--dry-run",
                "--json",
            ])
            .env("EXA_AGENT_NO_NETWORK", "1")
            .env("EXA_AGENT_PRESETS", &preset_path)
            .env("EXA_AGENT_LOCAL_PRESETS", directory.join("missing.toml"));
        if let Some(body) = &override_body {
            command.args(["--body", body]);
        }
        let output = command.output().unwrap();
        let result = stdout_json(&output);
        let expected = if override_body.is_some() {
            "row-1"
        } else {
            "preset"
        };
        assert_eq!(
            result["data"]["request"]["body"]["requests"][0]["customId"],
            expected
        );
        assert_eq!(
            result["data"]["request"]["body"]["metadata"]["origin"],
            "preset"
        );
    }
    fs::remove_dir_all(directory).unwrap();
}
