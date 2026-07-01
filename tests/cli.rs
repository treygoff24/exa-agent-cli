//! Parser tests for the v1 typed command tree (Wave 1A/1C skeleton).

use clap::Parser;
use exa_agent_cli::cli::{command_path, Cli, Command, SEARCH_TYPE_VALUES};
use exa_agent_cli::registry::{self, ConfirmProtocol};
use exa_agent_cli::transport;
use std::fs;
use std::io::{BufRead, ErrorKind, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

fn parses(args: &[&str]) -> Cli {
    let argv: Vec<String> = std::iter::once("exa-agent")
        .chain(args.iter().copied())
        .map(String::from)
        .collect();
    Cli::try_parse_from(argv).unwrap_or_else(|e| panic!("failed to parse {:?}: {e}", args))
}

fn parse_err(args: &[&str]) -> clap::Error {
    let argv: Vec<String> = std::iter::once("exa-agent")
        .chain(args.iter().copied())
        .map(String::from)
        .collect();
    Cli::try_parse_from(argv).unwrap_err()
}

fn assert_path(args: &[&str], expected: &str) {
    let cli = parses(args);
    assert_eq!(command_path(&cli.command), expected);
}

fn run(args: &[&str]) -> Output {
    run_with_env(args, &[])
}

fn run_owned(args: &[String]) -> Output {
    let mut cmd = command(&[]);
    cmd.args(args);
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to run exa-agent {args:?}: {e}"))
}

fn command(args: &[&str]) -> ProcessCommand {
    let mut cmd = ProcessCommand::new(env!("CARGO_BIN_EXE_exa-agent"));
    cmd.args(args)
        .env_remove("EXA_OUTPUT")
        .env_remove("EXA_API_KEY")
        .env_remove("EXA_SERVICE_KEY")
        .env_remove("EXA_ADMIN_BASE_URL")
        .env_remove("EXA_AGENT_CREDENTIALS")
        .env_remove("EXA_AGENT_CONFIG")
        .env_remove("EXA_PROFILE");
    cmd
}

fn run_with_env(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = command(args);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to run exa-agent {args:?}: {e}"))
}

fn run_with_env_stdin(args: &[&str], envs: &[(&str, &str)], stdin: &str) -> Output {
    let mut cmd = command(args);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn exa-agent {args:?}: {e}"));
    child
        .stdin
        .as_mut()
        .expect("stdin pipe")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    child
        .wait_with_output()
        .unwrap_or_else(|e| panic!("failed to wait for exa-agent {args:?}: {e}"))
}

fn run_ok_json(args: &[&str]) -> serde_json::Value {
    let output = run(args);
    assert!(
        output.status.success(),
        "expected success for {args:?}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("stdout was not JSON for {args:?}: {e}"))
}

fn stderr_json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stderr).unwrap_or_else(|e| panic!("stderr was not JSON: {e}"))
}

fn assert_confirmation_required(output: &Output, command: &str) -> serde_json::Value {
    assert_eq!(
        output.status.code(),
        Some(9),
        "{command} should refuse without confirmation\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "{command} wrote stdout on refusal"
    );
    let stderr = stderr_json(output);
    assert_eq!(
        stderr["error"]["code"], "confirmation_required",
        "{command}"
    );
    stderr
}

fn destructive_refusal_args(command: &str) -> Option<Vec<&'static str>> {
    Some(match command {
        "agent runs delete" => vec!["agent", "runs", "delete", "agent_run_abc", "--compact"],
        "monitor delete" => vec!["monitor", "delete", "mon_abc", "--compact"],
        "websets cancel" => vec!["websets", "cancel", "ws_abc", "--compact"],
        "websets delete" => vec!["websets", "delete", "ws_abc", "--compact"],
        "websets enrichments cancel" => vec![
            "websets",
            "enrichments",
            "cancel",
            "ws_abc",
            "enr_1",
            "--compact",
        ],
        "websets enrichments delete" => vec![
            "websets",
            "enrichments",
            "delete",
            "ws_abc",
            "enr_1",
            "--compact",
        ],
        "websets imports delete" => vec!["websets", "imports", "delete", "imp_abc", "--compact"],
        "websets items delete" => vec![
            "websets",
            "items",
            "delete",
            "ws_abc",
            "item_1",
            "--compact",
        ],
        "websets monitors delete" => {
            vec!["websets", "monitors", "delete", "mon_abc", "--compact"]
        }
        "websets searches cancel" => vec![
            "websets",
            "searches",
            "cancel",
            "ws_abc",
            "search_1",
            "--compact",
        ],
        "websets webhooks delete" => {
            vec!["websets", "webhooks", "delete", "wh_abc", "--compact"]
        }
        "admin keys delete" => vec!["admin", "keys", "delete", "key_abc", "--compact"],
        _ => return None,
    })
}

fn temp_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "exa-agent-cli-blackbox-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn closed_local_base_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    format!("http://{addr}")
}

fn http_request_lengths(buf: &[u8]) -> Option<(usize, usize)> {
    let header_end = buf.windows(4).position(|window| window == b"\r\n\r\n")? + 4;
    let headers = String::from_utf8_lossy(&buf[..header_end]);
    let content_len = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    Some((header_end, content_len))
}

fn read_http_request(stream: &mut std::net::TcpStream) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 1024];
    let started = Instant::now();
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(err)
                if err.kind() == ErrorKind::WouldBlock
                    && started.elapsed() < Duration::from_secs(10) =>
            {
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(err) => panic!("failed to read local test request: {err}"),
        }
        if let Some((header_end, content_len)) = http_request_lengths(&buf) {
            if buf.len() >= header_end + content_len {
                break;
            }
        }
    }
    buf
}

fn local_json_server<F>(
    validate: F,
    response_body: &'static [u8],
) -> (String, thread::JoinHandle<()>)
where
    F: FnOnce(String) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let started = Instant::now();
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(accepted) => break accepted,
                Err(err)
                    if err.kind() == ErrorKind::WouldBlock
                        && started.elapsed() < Duration::from_secs(10) =>
                {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(err) => panic!("failed to accept local test request: {err}"),
            }
        };
        validate(String::from_utf8_lossy(&read_http_request(&mut stream)).into_owned());
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response_body.len()
        )
        .unwrap();
        stream.write_all(response_body).unwrap();
    });
    (format!("http://{addr}"), server)
}

fn local_sse_stall_server(
    run_id: &str,
    event_data: &str,
) -> (String, mpsc::Sender<()>, thread::JoinHandle<()>) {
    let run_id = run_id.to_string();
    let event_data = event_data.to_string();
    let (stop_tx, stop_rx) = mpsc::channel();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let started = Instant::now();
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(accepted) => break accepted,
                Err(err)
                    if err.kind() == ErrorKind::WouldBlock
                        && started.elapsed() < Duration::from_secs(10) =>
                {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(err) => panic!("failed to accept local SSE test request: {err}"),
            }
        };
        let request = String::from_utf8_lossy(&read_http_request(&mut stream)).into_owned();
        assert!(
            request.starts_with(&format!("GET /agent/runs/{run_id}/events ")),
            "unexpected SSE request:\n{request}"
        );
        assert!(
            request
                .to_ascii_lowercase()
                .contains("accept: text/event-stream"),
            "expected SSE Accept header:\n{request}"
        );
        assert!(
            request
                .to_ascii_lowercase()
                .contains("last-event-id: evt-resume"),
            "expected Last-Event-ID replay header:\n{request}"
        );
        assert!(
            request
                .to_ascii_lowercase()
                .contains("x-api-key: test-key-abcdef12"),
            "expected test API key header:\n{request}"
        );
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\n\r\nid: evt-42\r\ndata: {event_data}\r\n\r\nid: evt-43\r\n"
        )
        .unwrap();
        stream.flush().unwrap();
        let _ = stop_rx.recv_timeout(Duration::from_secs(5));
    });
    (format!("http://{addr}"), stop_tx, server)
}

fn local_paginated_agent_runs_server() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let responses = [
            r#"{"data":[{"id":"agent_run_1"}],"hasMore":true,"nextCursor":"cur2"}"#,
            r#"{"data":[{"id":"agent_run_2"}],"hasMore":false,"nextCursor":null}"#,
        ];
        for (idx, response_body) in responses.iter().enumerate() {
            let started = Instant::now();
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(accepted) => break accepted,
                    Err(err)
                        if err.kind() == ErrorKind::WouldBlock
                            && started.elapsed() < Duration::from_secs(10) =>
                    {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(err) => panic!("failed to accept local pagination test request: {err}"),
                }
            };
            let request = String::from_utf8_lossy(&read_http_request(&mut stream)).into_owned();
            if idx == 0 {
                assert!(
                    request.starts_with("GET /agent/runs?limit=1 "),
                    "unexpected first page request:\n{request}"
                );
            } else {
                assert!(
                    request.starts_with("GET /agent/runs?limit=1&cursor=cur2 "),
                    "unexpected second page request:\n{request}"
                );
            }
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("x-api-key: test-key-abcdef12"),
                "expected test API key header:\n{request}"
            );
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            )
            .unwrap();
            stream.flush().unwrap();
        }
    });
    (format!("http://{addr}"), server)
}

#[test]
fn parse_capabilities_and_describe_alias() {
    assert_path(&["capabilities"], "capabilities");
    assert_path(&["describe"], "capabilities");
}

#[test]
fn parse_schema_commands() {
    assert_path(&["schema", "list"], "schema list");
    assert_path(&["schema", "show", "SearchRequest"], "schema show");
    parses(&[
        "schema",
        "export",
        "--api",
        "openapi",
        "--output",
        "exa-spec.yaml",
    ]);
    parses(&[
        "schema",
        "validate-input",
        "search",
        "--body",
        "@request.json",
    ]);
    parses(&["schema", "refresh", "--check"]);
}

#[test]
fn parse_robot_docs_commands() {
    assert_path(&["robot-docs", "guide"], "robot-docs guide");
    assert_path(&["robot-docs", "commands"], "robot-docs commands");
    assert_path(&["robot-docs", "errors"], "robot-docs errors");
    parses(&["robot-docs", "examples", "--task", "search"]);
    assert_path(&["robot-docs", "prompts"], "robot-docs prompts");
}

#[test]
fn schema_commands_work_offline() {
    let show = run_ok_json(&["schema", "show", "search", "--compact"]);
    assert_eq!(show["schema"], "exa.cli.schema_show.v1");
    assert_eq!(show["ok"], true);
    assert_eq!(show["operation"]["command"], "search");

    let export = run_ok_json(&["schema", "export", "--api", "openapi", "--compact"]);
    assert_eq!(export["schema"], "exa.cli.schema_export.v1");
    assert_eq!(export["target"], "openapi");
    assert!(!export["operations"].as_array().unwrap().is_empty());

    let validate = run_ok_json(&[
        "schema",
        "validate-input",
        "search",
        "--body",
        r#"{"query":"x"}"#,
        "--compact",
    ]);
    assert_eq!(validate["schema"], "exa.cli.schema_validate_input.v1");
    assert_eq!(validate["valid"], true);

    let templated_array = run_ok_json(&[
        "schema",
        "validate-input",
        "websets searches create",
        "--body",
        r#"{"query":"founders","count":25,"criteria":[{"description":"has email"}]}"#,
        "--compact",
    ]);
    assert_eq!(templated_array["valid"], true);

    let wrong_template_shape = run_ok_json(&[
        "schema",
        "validate-input",
        "websets searches create",
        "--body",
        r#"{"query":"founders","count":25,"criteria":["has email"]}"#,
        "--compact",
    ]);
    assert_eq!(wrong_template_shape["valid"], false);
    assert_eq!(
        wrong_template_shape["details"]["issue"],
        "invalid_field_type"
    );
    assert_eq!(wrong_template_shape["details"]["field"], "criteria");

    let missing_query = run_ok_json(&[
        "schema",
        "validate-input",
        "search",
        "--body",
        r#"{"numResults":5}"#,
        "--compact",
    ]);
    assert_eq!(missing_query["valid"], false);
    assert_eq!(missing_query["details"]["field"], "query");
    assert!(missing_query["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--num-results 5"));

    let invalid_type = run_ok_json(&[
        "schema",
        "validate-input",
        "search",
        "--body",
        r#"{"query":"x","type":"bogus"}"#,
        "--compact",
    ]);
    assert_eq!(invalid_type["valid"], false);
    assert_eq!(invalid_type["details"]["field"], "type");
    assert_eq!(invalid_type["details"]["issue"], "invalid_enum_value");
    assert_eq!(
        invalid_type["details"]["allowed"],
        serde_json::json!(SEARCH_TYPE_VALUES)
    );

    let unsupported = run_ok_json(&[
        "schema",
        "validate-input",
        "team info",
        "--body",
        "{}",
        "--compact",
    ]);
    assert!(unsupported["valid"].is_null());
    assert!(unsupported["note"]
        .as_str()
        .unwrap()
        .contains("unsupported"));

    let refresh = run_ok_json(&["schema", "refresh", "--check", "--compact"]);
    assert_eq!(refresh["schema"], "exa.cli.schema_refresh.v1");
    assert_eq!(refresh["status"], "current");
}

#[test]
fn robot_docs_commands_work_offline() {
    for (args, section) in [
        (vec!["robot-docs", "guide", "--compact"], "guide"),
        (vec!["robot-docs", "commands", "--compact"], "commands"),
        (vec!["robot-docs", "errors", "--compact"], "errors"),
        (
            vec!["robot-docs", "examples", "--task", "search", "--compact"],
            "examples",
        ),
        (vec!["robot-docs", "prompts", "--compact"], "prompts"),
    ] {
        let json = run_ok_json(&args);
        assert_eq!(json["schema"], "exa.cli.robot_docs.v1");
        assert_eq!(json["ok"], true);
        assert_eq!(json["section"], section);
    }
}

#[test]
fn parse_config_commands() {
    assert_path(&["config", "list"], "config list");
    assert_path(&["config", "get", "base-url"], "config get");
    assert_path(
        &["config", "set", "base-url", "https://api.exa.ai"],
        "config set",
    );
    assert_path(&["config", "unset", "base-url"], "config unset");
    assert_path(&["config", "path"], "config path");
    assert_path(&["config", "profiles", "list"], "config profiles list");
    assert_path(
        &["config", "profiles", "show", "default"],
        "config profiles show",
    );
    assert_path(
        &["config", "profiles", "use", "work"],
        "config profiles use",
    );
    assert_path(
        &["config", "profiles", "create", "staging"],
        "config profiles create",
    );
    assert_path(
        &["config", "profiles", "delete", "staging"],
        "config profiles delete",
    );
}

#[test]
fn parse_auth_commands() {
    assert_path(&["auth", "status"], "auth status");
    assert_path(&["auth", "test"], "auth test");
    assert_path(&["auth", "login"], "auth login");
    assert_path(&["auth", "logout"], "auth logout");
    assert_eq!(
        parse_err(&["--api-key", "key", "--api-key-stdin", "auth", "status"]).kind(),
        clap::error::ErrorKind::ArgumentConflict
    );
    assert_eq!(
        parse_err(&["--api-key-stdin", "--service-key-stdin", "auth", "status"]).kind(),
        clap::error::ErrorKind::ArgumentConflict
    );
}

#[test]
fn auth_help_does_not_claim_keyring_storage() {
    let output = run(&["auth", "--help"]);
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    assert!(
        !help.to_ascii_lowercase().contains("keyring"),
        "auth help should not claim OS keyring storage: {help}"
    );
    assert!(help.contains("credentials file"));
}

#[test]
fn auth_test_without_credential_is_not_authenticated() {
    let dir = temp_path("auth-test-no-credential");
    let missing_credentials = dir.join("missing-credentials.json");
    let output = run_with_env(
        &["auth", "test", "--compact"],
        &[(
            "EXA_AGENT_CREDENTIALS",
            missing_credentials.to_str().unwrap(),
        )],
    );
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = stderr_json(&output);
    assert_eq!(stderr["error"]["code"], "not_authenticated");
}

#[test]
fn auth_test_dry_run_previews_without_touching_network() {
    // A live probe must still honor --dry-run: no network, just a preview. A fake key
    // would 404 against real Exa if the network were hit; the gate runs before that.
    let output = run_with_env(
        &["auth", "test", "--dry-run", "--print-request", "--compact"],
        &[("EXA_API_KEY", "test-fake-key")],
    );
    assert_eq!(output.status.code(), Some(0));
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(stdout["dryRun"], true);
    assert_eq!(stdout["endpoint"], "/v0/teams/me");
    // Proof it never reached the network: no upstream HTTP status / HTML body leaked in.
    let raw = String::from_utf8_lossy(&output.stdout);
    assert!(!raw.contains("httpStatus") && !raw.contains("DOCTYPE"));
}

#[test]
fn validate_input_rejects_wrong_type_and_out_of_range() {
    // Registry FieldKind type check: numResults is Int, a string fails.
    let wrong_type = run(&[
        "schema",
        "validate-input",
        "search",
        "--body",
        "{\"query\":\"x\",\"numResults\":\"five\"}",
        "--compact",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&wrong_type.stdout).unwrap();
    assert_eq!(v["valid"], false);
    assert_eq!(v["details"]["issue"], "invalid_field_type");

    // Reuses the live range validator: 500 is out of the 1..=100 range.
    let out_of_range = run(&[
        "schema",
        "validate-input",
        "search",
        "--body",
        "{\"query\":\"x\",\"numResults\":500}",
        "--compact",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&out_of_range.stdout).unwrap();
    assert_eq!(v["valid"], false);

    // A structurally valid, in-range body still passes.
    let ok = run(&[
        "schema",
        "validate-input",
        "search",
        "--body",
        "{\"query\":\"x\",\"numResults\":5}",
        "--compact",
    ]);
    let v: serde_json::Value = serde_json::from_slice(&ok.stdout).unwrap();
    assert_eq!(v["valid"], true);
}

#[test]
fn parse_doctor() {
    assert_path(&["doctor"], "doctor");
    parses(&["doctor", "--online"]);
}

#[test]
fn parse_raw() {
    assert_path(&["raw", "POST", "/search"], "raw");
    parses(&["raw", "GET", "/v0/websets", "--body", "@req.json"]);
    parses(&[
        "raw",
        "GET",
        "/v0/websets",
        "--query",
        "status=running",
        "--query",
        "limit=10",
    ]);
}

#[test]
fn raw_dry_run_includes_query_preview() {
    let json = run_ok_json(&[
        "raw",
        "GET",
        "/v0/websets",
        "--query",
        "status=running",
        "--query",
        "limit=10",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["schema"], "exa.cli.response.v1");
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "raw");
    assert!(json["dataHash"].as_str().unwrap().starts_with("sha256:"));
    assert_eq!(json["data"]["request"]["method"], "GET");
    assert_eq!(json["data"]["request"]["path"], "/v0/websets");
    assert_eq!(
        json["data"]["request"]["query"],
        serde_json::json!([
            { "name": "status", "value": "running" },
            { "name": "limit", "value": "10" }
        ])
    );
}

#[test]
fn raw_dry_run_redacts_secret_query_values() {
    let json = run_ok_json(&[
        "raw",
        "GET",
        "/v0/websets",
        "--query",
        "api_key=query-secret",
        "--query",
        "status=running",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(
        json["data"]["request"]["query"],
        serde_json::json!([
            { "name": "api_key", "value": "<redacted>" },
            { "name": "status", "value": "running" }
        ])
    );

    let json = run_ok_json(&[
        "raw",
        "GET",
        "/v0/websets",
        "--query",
        "q=11111111-2222-3333-4444-555555555555",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(
        json["data"]["request"]["query"],
        serde_json::json!([{ "name": "q", "value": "<redacted>" }])
    );
}

#[test]
fn auth_status_uses_credentials_file_without_leaking_secret() {
    let dir = temp_path("auth-status");
    let credentials = dir.join("credentials.json");
    fs::write(&credentials, r#"{"api_key":"file-secret-1234"}"#).unwrap();
    let output = run_with_env(
        &["auth", "status", "--compact"],
        &[("EXA_AGENT_CREDENTIALS", credentials.to_str().unwrap())],
    );
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("file-secret-1234"));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["schema"], "exa.cli.auth_status.v1");
    assert_eq!(json["authenticated"], true);
    assert!(json["source"].as_str().unwrap().starts_with("file:"));
    assert_eq!(json["last4"], "1234");
}

#[test]
fn auth_login_and_logout_use_isolated_credentials_file() {
    let dir = temp_path("auth-login");
    let credentials = dir.join("credentials.json");
    let envs = [("EXA_AGENT_CREDENTIALS", credentials.to_str().unwrap())];
    let login = run_with_env_stdin(&["auth", "login", "--compact"], &envs, "login-secret-9999");
    assert!(login.status.success());
    let stdout = String::from_utf8_lossy(&login.stdout);
    assert!(!stdout.contains("login-secret-9999"));
    assert!(fs::read_to_string(&credentials)
        .unwrap()
        .contains("login-secret-9999"));

    let logout = run_with_env(&["auth", "logout", "--compact"], &envs);
    assert!(logout.status.success());
    let remaining = fs::read_to_string(&credentials).unwrap_or_default();
    assert!(!remaining.contains("login-secret-9999"));
}

#[test]
fn config_set_get_roundtrip_uses_config_override() {
    let dir = temp_path("config-roundtrip");
    let config = dir.join("config.toml");
    let envs = [("EXA_AGENT_CONFIG", config.to_str().unwrap())];
    let set = run_with_env(
        &[
            "config",
            "set",
            "base-url",
            "https://example.com",
            "--compact",
        ],
        &envs,
    );
    assert!(set.status.success());
    let get = run_with_env(&["config", "get", "base-url", "--compact"], &envs);
    assert!(get.status.success());
    let json: serde_json::Value = serde_json::from_slice(&get.stdout).unwrap();
    assert_eq!(json["schema"], "exa.cli.config_get.v1");
    assert_eq!(json["value"], "https://example.com");
}

#[test]
fn doctor_malformed_config_reports_finding_on_stdout_exit_one() {
    let dir = temp_path("doctor-bad-config");
    let config = dir.join("config.toml");
    fs::write(&config, "not = valid toml [[[\\n").unwrap();
    let output = run_with_env(
        &["doctor", "--compact"],
        &[("EXA_AGENT_CONFIG", config.to_str().unwrap())],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["schema"], "exa.cli.doctor.v1");
    assert_eq!(json["status"], "findings");
    assert!(json["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| finding["id"] == "config.parse" && finding["status"] == "fail"));
}

#[test]
fn doctor_warn_findings_exit_one() {
    let dir = temp_path("doctor-warn");
    let config = dir.join("config.toml");
    let credentials = dir.join("missing-credentials.json");
    fs::write(&config, "base_url = \"https://api.exa.ai\"\n").unwrap();
    let output = run_with_env(
        &["doctor", "--check", "key.present", "--compact"],
        &[
            ("EXA_AGENT_CONFIG", config.to_str().unwrap()),
            ("EXA_AGENT_CREDENTIALS", credentials.to_str().unwrap()),
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "findings");
    assert_eq!(json["ok"], false);
    assert!(json["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| finding["id"] == "key.present" && finding["status"] == "warn"));
}

#[test]
fn doctor_unknown_check_is_usage_error() {
    let output = run(&["doctor", "--check", "key.presnt", "--compact"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");
    assert!(stderr["error"]["details"]["valid"].is_array());
}

#[test]
fn doctor_key_present_detector_uses_credentials_file_without_leaking_secret() {
    let dir = temp_path("doctor-key-file");
    let credentials = dir.join("credentials.json");
    fs::write(&credentials, r#"{"api_key":"doctor-secret-5555"}"#).unwrap();
    let output = run_with_env(
        &["doctor", "--check", "key.present", "--compact"],
        &[("EXA_AGENT_CREDENTIALS", credentials.to_str().unwrap())],
    );
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("doctor-secret-5555"));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "healthy");
    assert!(json["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| finding["id"] == "key.present" && finding["status"] == "ok"));
}

#[test]
fn config_errors_redact_secret_shaped_values() {
    let dir = temp_path("config-error-redaction");
    let config = dir.join("config.toml");
    let output = run_with_env(
        &[
            "config",
            "set",
            "base-url",
            "exa-secret-config-1234",
            "--compact",
        ],
        &[("EXA_AGENT_CONFIG", config.to_str().unwrap())],
    );
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("exa-secret-config-1234"));
    assert!(stderr.contains("<redacted>"));
}

#[test]
fn config_rejects_secret_shaped_key_env_and_malformed_base_url() {
    let dir = temp_path("config-validation");
    let config = dir.join("config.toml");
    let envs = [("EXA_AGENT_CONFIG", config.to_str().unwrap())];

    let profile = run_with_env(
        &["config", "profiles", "create", "work", "--compact"],
        &envs,
    );
    assert!(profile.status.success());

    let secret_env = run_with_env(
        &[
            "config",
            "set",
            "profiles.work.api-key-env",
            "sk-exa-secret-1234",
            "--compact",
        ],
        &envs,
    );
    assert_eq!(secret_env.status.code(), Some(3));
    let secret_stderr = String::from_utf8_lossy(&secret_env.stderr);
    assert!(!secret_stderr.contains("sk-exa-secret-1234"));
    assert!(!fs::read_to_string(&config)
        .unwrap()
        .contains("sk-exa-secret-1234"));

    let bad_url = run_with_env(
        &[
            "config",
            "set",
            "base-url",
            "https://not a url",
            "--compact",
        ],
        &envs,
    );
    assert_eq!(bad_url.status.code(), Some(3));
}

#[test]
fn doctor_redacts_secret_shaped_config_values() {
    let dir = temp_path("doctor-redaction");
    let config = dir.join("config.toml");
    fs::write(&config, "base_url = \"https://exa-secret-base-1234\"\n").unwrap();
    let output = run_with_env(
        &["doctor", "--check", "base-url", "--compact"],
        &[("EXA_AGENT_CONFIG", config.to_str().unwrap())],
    );
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("exa-secret-base-1234"));
    assert!(stdout.contains("<redacted>"));
}

#[test]
fn auth_status_rejects_api_shaped_service_key_for_admin_capability() {
    let output = run_with_env(
        &["auth", "status", "--compact"],
        &[("EXA_SERVICE_KEY", "exa-api-shaped-service-key")],
    );
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["canAdmin"], false);
    assert!(json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning
            .as_str()
            .unwrap_or_default()
            .contains("admin commands require a service key")));
}

#[cfg(unix)]
#[test]
fn auth_login_with_credentials_override_does_not_chmod_parent_dir() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let dir = temp_path("auth-login-perms");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    let before = fs::metadata(&dir).unwrap().mode() & 0o777;
    let credentials = dir.join("credentials.json");
    let output = run_with_env_stdin(
        &["auth", "login", "--compact"],
        &[("EXA_AGENT_CREDENTIALS", credentials.to_str().unwrap())],
        "permission-secret-1234",
    );
    assert!(output.status.success());
    let after = fs::metadata(&dir).unwrap().mode() & 0o777;
    let file_mode = fs::metadata(&credentials).unwrap().mode() & 0o777;
    assert_eq!(before, 0o755);
    assert_eq!(after, 0o755);
    assert_eq!(file_mode, 0o600);
}

#[test]
fn search_dry_run_merges_body_and_set_with_redaction() {
    let json = run_ok_json(&[
        "search",
        "named query",
        "--body",
        r#"{"numResults":10,"token":"body-secret","contents":{"summary":{"query":"body summary"}}}"#,
        "--set",
        "contents.text=true",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["schema"], "exa.cli.response.v1");
    assert_eq!(json["command"], "search");
    assert!(json["dataHash"].as_str().unwrap().starts_with("sha256:"));
    let body = &json["data"]["request"]["body"];
    assert_eq!(body["query"], "named query");
    assert_eq!(body["numResults"], 10);
    assert_eq!(body["contents"]["summary"]["query"], "body summary");
    assert_eq!(body["contents"]["text"], true);
    assert_eq!(body["token"], "<redacted>");
}

#[test]
fn raw_dry_run_reads_body_and_set_then_redacts() {
    let json = run_ok_json(&[
        "raw",
        "POST",
        "/custom",
        "--body",
        r#"{"query":"keep","password":"body-secret"}"#,
        "--set",
        "token=set-secret",
        "--dry-run",
        "--compact",
    ]);
    let body = &json["data"]["request"]["body"];
    assert_eq!(body["query"], "keep");
    assert_eq!(body["password"], "<redacted>");
    assert_eq!(body["token"], "<redacted>");
}

#[test]
fn raw_refuses_user_authorization_header_before_auth() {
    let output = run(&[
        "--header",
        "Authorization: Bearer user-supplied-secret",
        "raw",
        "GET",
        "/search",
        "--compact",
    ]);
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr)
        .unwrap_or_else(|e| panic!("stderr was not JSON: {e}"));
    assert_eq!(stderr["schema"], "exa.cli.error.v1");
    assert_eq!(stderr["error"]["code"], "invalid_flag_combination");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!all.contains("user-supplied-secret"));
}

#[test]
fn raw_dry_run_refuses_user_authorization_header() {
    let output = run(&[
        "--header",
        "Authorization: Bearer user-supplied-secret",
        "raw",
        "GET",
        "/search",
        "--dry-run",
        "--compact",
    ]);
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr)
        .unwrap_or_else(|e| panic!("stderr was not JSON: {e}"));
    assert_eq!(stderr["error"]["code"], "invalid_flag_combination");
    assert_eq!(stderr["operation"]["method"], "GET");
    assert_eq!(stderr["operation"]["path"], "/search");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!all.contains("user-supplied-secret"));
}

#[test]
fn raw_live_without_credential_is_not_authenticated() {
    let dir = temp_path("raw-no-credential");
    let missing_credentials = dir.join("missing-credentials.json");
    let output = run_with_env(
        &[
            "--correlation-id",
            "corr-raw-no-credential",
            "raw",
            "GET",
            "/search",
            "--compact",
        ],
        &[(
            "EXA_AGENT_CREDENTIALS",
            missing_credentials.to_str().unwrap(),
        )],
    );
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr)
        .unwrap_or_else(|e| panic!("stderr was not JSON: {e}"));
    assert_eq!(stderr["schema"], "exa.cli.error.v1");
    assert_eq!(stderr["error"]["code"], "not_authenticated");
    assert_eq!(stderr["operation"]["method"], "GET");
    assert_eq!(stderr["operation"]["path"], "/search");
    assert_eq!(stderr["request"]["correlationId"], "corr-raw-no-credential");
    assert!(stderr["request"]["requestId"]
        .as_str()
        .unwrap()
        .starts_with("req_local_"));
    assert!(stderr["error"]["details"]["checked"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value.as_str().unwrap_or("").contains("credentials")));
}

#[test]
fn raw_error_context_redacts_secret_shaped_path() {
    let dir = temp_path("raw-redact-path");
    let missing_credentials = dir.join("missing-credentials.json");
    let output = run_with_env(
        &[
            "--correlation-id",
            "11111111-2222-3333-4444-555555555555",
            "raw",
            "GET",
            "/search/11111111-2222-3333-4444-555555555555",
            "--compact",
        ],
        &[(
            "EXA_AGENT_CREDENTIALS",
            missing_credentials.to_str().unwrap(),
        )],
    );
    assert!(!output.status.success());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr)
        .unwrap_or_else(|e| panic!("stderr was not JSON: {e}"));
    assert_eq!(stderr["operation"]["path"], "/search/<redacted>");
    assert_eq!(stderr["request"]["correlationId"], "<redacted>");
    let all = String::from_utf8_lossy(&output.stderr);
    assert!(!all.contains("11111111-2222-3333-4444-555555555555"));
}

#[test]
fn raw_malformed_inputs_do_not_echo_secret_values() {
    let output = run(&[
        "--header",
        "X-Trace 11111111-2222-3333-4444-555555555555",
        "raw",
        "GET",
        "/search",
        "--dry-run",
        "--compact",
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("`--header` must be `Name: value`"));
    assert!(!stderr.contains("11111111-2222-3333-4444-555555555555"));

    let output = run(&[
        "raw",
        "GET",
        "/search",
        "--query",
        "q 11111111-2222-3333-4444-555555555555",
        "--dry-run",
        "--compact",
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("raw --query expects `key=value`"));
    assert!(!stderr.contains("11111111-2222-3333-4444-555555555555"));
}

#[test]
fn raw_body_stdin_empty_returns_no_input() {
    let output = run(&["raw", "POST", "/custom", "--body", "-", "--dry-run"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr)
        .unwrap_or_else(|e| panic!("stderr was not JSON: {e}"));
    assert_eq!(stderr["error"]["code"], "no_input");
    assert_eq!(stderr["error"]["category"], "no_input");
}

#[test]
fn search_live_without_credential_is_not_authenticated() {
    let dir = temp_path("search-no-credential");
    let missing_credentials = dir.join("missing-credentials.json");
    let output = run_with_env(
        &["search", "agents", "--compact"],
        &[(
            "EXA_AGENT_CREDENTIALS",
            missing_credentials.to_str().unwrap(),
        )],
    );
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "not_authenticated");
    assert_eq!(stderr["operation"]["method"], "POST");
    assert_eq!(stderr["operation"]["path"], "/search");
}

#[test]
fn chunked_contents_live_without_credential_keeps_operation_context() {
    let dir = temp_path("contents-chunked-no-credential");
    let missing_credentials = dir.join("missing-credentials.json");
    let output = run_with_env(
        &[
            "contents",
            "https://a.test",
            "https://b.test",
            "--chunk-size",
            "1",
            "--compact",
        ],
        &[(
            "EXA_AGENT_CREDENTIALS",
            missing_credentials.to_str().unwrap(),
        )],
    );
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "not_authenticated");
    assert_eq!(stderr["operation"]["method"], "POST");
    assert_eq!(stderr["operation"]["path"], "/contents");
}

#[test]
fn set_overflow_path_returns_structured_error_not_panic() {
    let output = run(&[
        "search",
        "q",
        "--set",
        "18446744073709551615=x",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr)
        .unwrap_or_else(|e| panic!("stderr was not JSON: {e}"));
    assert_eq!(stderr["error"]["code"], "invalid_value");
}

#[test]
fn contents_dry_run_builds_urls_and_ids_bodies() {
    let urls = run_ok_json(&[
        "contents",
        "https://exa.ai",
        "https://docs.exa.ai",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(urls["schema"], "exa.cli.response.v1");
    assert_eq!(urls["command"], "contents");
    assert_eq!(urls["data"]["request"]["method"], "POST");
    assert_eq!(urls["data"]["request"]["path"], "/contents");
    assert_eq!(
        urls["data"]["request"]["body"]["urls"],
        serde_json::json!(["https://exa.ai", "https://docs.exa.ai"])
    );

    let ids = run_ok_json(&[
        "contents",
        "--ids",
        "id-one",
        "id-two",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(ids["command"], "contents");
    assert_eq!(
        ids["data"]["request"]["body"]["ids"],
        serde_json::json!(["id-one", "id-two"])
    );
}

#[test]
fn contents_chunk_size_dry_run_emits_one_envelope_per_chunk() {
    let output = run(&[
        "contents",
        "https://a.test",
        "https://b.test",
        "--chunk-size",
        "1",
        "--dry-run",
        "--compact",
    ]);
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "stdout:\n{stdout}");
    let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    let second: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(
        first["data"]["request"]["body"]["urls"],
        serde_json::json!(["https://a.test"])
    );
    assert_eq!(
        second["data"]["request"]["body"]["urls"],
        serde_json::json!(["https://b.test"])
    );
}

#[test]
fn contents_rejects_more_than_one_hundred_inputs_without_chunk_size() {
    let mut args = vec!["contents".to_string()];
    for n in 0..101 {
        args.push(format!("https://example.test/{n}"));
    }
    args.push("--dry-run".to_string());
    args.push("--compact".to_string());

    let output = run_owned(&args);
    assert_eq!(
        output.status.code(),
        Some(1),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_flag_combination");
    assert_eq!(stderr["operation"]["method"], "POST");
    assert_eq!(stderr["operation"]["path"], "/contents");
    assert!(stderr["error"]["suggestedCommand"]
        .as_str()
        .unwrap_or_default()
        .contains("--chunk-size 100"));
}

#[test]
fn answer_dry_run_builds_request_body_with_schema_file() {
    let dir = temp_path("answer-schema");
    let schema_path = dir.join("answer.schema.json");
    fs::write(
        &schema_path,
        r#"{"type":"object","properties":{"answer":{"type":"string"}}}"#,
    )
    .unwrap();
    let schema_arg = format!("@{}", schema_path.display());

    let output = run_owned(&[
        "answer".into(),
        "What is Exa?".into(),
        "--text".into(),
        "--stream".into(),
        "--output-schema".into(),
        schema_arg,
        "--body".into(),
        r#"{"text":false}"#.into(),
        "--set".into(),
        "stream=false".into(),
        "--dry-run".into(),
        "--compact".into(),
    ]);
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let body = &json["data"]["request"]["body"];
    assert_eq!(json["command"], "answer");
    assert_eq!(json["data"]["request"]["path"], "/answer");
    assert_eq!(body["query"], "What is Exa?");
    assert_eq!(body["text"], false);
    assert_eq!(body["stream"], false);
    assert_eq!(
        body["outputSchema"],
        serde_json::json!({"type":"object","properties":{"answer":{"type":"string"}}})
    );
}

#[test]
fn context_dry_run_omits_dynamic_tokens_and_validates_range() {
    let dynamic = run_ok_json(&[
        "context",
        "rust async patterns",
        "--tokens",
        "dynamic",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(dynamic["command"], "context");
    assert_eq!(dynamic["data"]["request"]["path"], "/context");
    assert_eq!(
        dynamic["data"]["request"]["body"],
        serde_json::json!({"query":"rust async patterns"})
    );

    let fixed = run_ok_json(&[
        "context",
        "rust async patterns",
        "--tokens",
        "1000",
        "--body",
        r#"{"tokensNum":2000}"#,
        "--set",
        "tokensNum=3000",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(fixed["data"]["request"]["body"]["tokensNum"], 3000);

    for bad in ["49", "100001", "lots"] {
        let output = run(&["context", "q", "--tokens", bad, "--dry-run", "--compact"]);
        assert_eq!(
            output.status.code(),
            Some(1),
            "bad={bad}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(stderr["error"]["code"], "invalid_value");
        assert_eq!(stderr["operation"]["path"], "/context");
    }
}

#[test]
fn context_rejects_queries_over_two_thousand_chars() {
    let ok_query = "x".repeat(2_000);
    let ok = run_owned(&[
        "context".into(),
        ok_query,
        "--dry-run".into(),
        "--compact".into(),
    ]);
    assert!(ok.status.success());

    let too_long = "x".repeat(2_001);
    let output = run_owned(&[
        "context".into(),
        too_long,
        "--dry-run".into(),
        "--compact".into(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");
    assert!(stderr["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .contains("2000"));
    assert_eq!(stderr["operation"]["path"], "/context");

    let body_override = run_owned(&[
        "context".into(),
        "short".into(),
        "--body".into(),
        serde_json::json!({ "query": "x".repeat(2_001) }).to_string(),
        "--dry-run".into(),
        "--compact".into(),
    ]);
    assert_eq!(body_override.status.code(), Some(1));
    assert!(body_override.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&body_override.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");
    assert_eq!(stderr["operation"]["path"], "/context");

    let set_override = run_owned(&[
        "context".into(),
        "short".into(),
        "--set".into(),
        format!("query={}", "x".repeat(2_001)),
        "--dry-run".into(),
        "--compact".into(),
    ]);
    assert_eq!(set_override.status.code(), Some(1));
    assert!(set_override.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&set_override.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");
    assert_eq!(stderr["operation"]["path"], "/context");
}

#[test]
fn similar_dry_run_builds_request_body() {
    let json = run_ok_json(&[
        "similar",
        "https://exa.ai",
        "--num-results",
        "7",
        "--exclude-source-domain",
        "--category",
        "company",
        "--body",
        r#"{"numResults":8}"#,
        "--set",
        "category=people",
        "--dry-run",
        "--compact",
    ]);
    let body = &json["data"]["request"]["body"];
    assert_eq!(json["command"], "similar");
    assert_eq!(json["data"]["request"]["path"], "/findSimilar");
    assert_eq!(body["url"], "https://exa.ai");
    assert_eq!(body["numResults"], 8);
    assert_eq!(body["excludeSourceDomain"], true);
    assert_eq!(body["category"], "people");
    assert_eq!(json["warnings"][0]["code"], "deprecated_upstream");
}

#[test]
fn team_info_dry_run_builds_get_path() {
    let json = run_ok_json(&["team", "info", "--dry-run", "--print-request", "--compact"]);
    assert_eq!(json["command"], "team info");
    assert_eq!(json["data"]["request"]["method"], "GET");
    assert_eq!(json["data"]["request"]["path"], "/v0/teams/me");
    assert_eq!(json["data"]["request"]["query"], serde_json::json!([]));
    assert_eq!(json["data"]["request"]["body"], serde_json::json!(null));
    assert_eq!(json["data"]["dryRun"], true);
}

#[test]
fn research_dry_run_builds_create_list_and_get_requests() {
    let create = run_ok_json(&[
        "research",
        "create",
        "legacy topic",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(create["command"], "research create");
    assert_eq!(create["data"]["request"]["path"], "/research/v1");
    assert_eq!(
        create["data"]["request"]["body"]["instructions"],
        "legacy topic"
    );
    assert_eq!(create["warnings"][0]["code"], "legacy_api");

    let list = run_ok_json(&[
        "research",
        "list",
        "--limit",
        "10",
        "--cursor",
        "cur_abc",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(list["command"], "research list");
    assert_eq!(list["data"]["request"]["path"], "/research/v1");
    assert_eq!(
        list["data"]["request"]["query"],
        serde_json::json!([
            {"name": "limit", "value": "10"},
            {"name": "cursor", "value": "cur_abc"}
        ])
    );
    assert_eq!(list["data"]["request"]["body"], serde_json::json!(null));
    assert_eq!(list["warnings"][0]["code"], "legacy_api");

    let get = run_ok_json(&["research", "get", "research/abc", "--dry-run", "--compact"]);
    assert_eq!(get["command"], "research get");
    assert_eq!(
        get["data"]["request"]["path"],
        "/research/v1/research%2Fabc"
    );
    assert_eq!(get["data"]["request"]["body"], serde_json::json!(null));
    assert_eq!(get["warnings"][0]["code"], "legacy_api");
}

#[test]
fn research_accepts_all_but_rejects_orphaned_pagination_flags_and_create_stream() {
    let list_all = run_ok_json(&["research", "list", "--all", "--dry-run", "--compact"]);
    assert_eq!(list_all["command"], "research list");
    assert_eq!(list_all["data"]["request"]["path"], "/research/v1");
    assert_eq!(list_all["data"]["request"]["query"], serde_json::json!([]));

    let max_pages = run(&[
        "research",
        "list",
        "--max-pages",
        "3",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(max_pages.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&max_pages.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_flag_combination");
    assert!(stderr["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--all"));

    let page_delay = run(&[
        "research",
        "list",
        "--page-delay",
        "100ms",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(page_delay.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&page_delay.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_flag_combination");

    let max_pages_zero = run(&[
        "research",
        "list",
        "--all",
        "--max-pages",
        "0",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(max_pages_zero.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&max_pages_zero.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");

    let create_stream = run(&[
        "research",
        "create",
        "legacy topic",
        "--stream",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(create_stream.status.code(), Some(1));
    assert!(create_stream.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&create_stream.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_flag_combination");
    assert_eq!(stderr["operation"]["path"], "/research/v1");
}

#[test]
fn parse_search_core() {
    assert_path(&["search", "latest AI chip launches"], "search");
    parses(&[
        "search",
        "query",
        "-n",
        "5",
        "--type",
        "Fast",
        "--category",
        "research paper",
    ]);
    assert_eq!(
        parse_err(&["search", "query", "--type", "garbage"]).kind(),
        clap::error::ErrorKind::InvalidValue
    );
}

#[test]
fn clap_missing_required_argument_names_query() {
    let output = run(&["search"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = stderr_json(&output);
    assert_eq!(stderr["error"]["code"], "missing_required_argument");
    assert!(stderr["error"]["message"]
        .as_str()
        .unwrap()
        .contains("<QUERY>"));
    let missing = stderr["error"]["details"]["missing"].as_array().unwrap();
    assert!(!missing.is_empty());
    assert!(missing
        .iter()
        .any(|value| value.as_str().is_some_and(|value| value.contains("QUERY"))));
}

#[test]
fn clap_unknown_subcommand_includes_did_you_mean() {
    let output = run(&["serch", "x"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = stderr_json(&output);
    assert_eq!(stderr["error"]["code"], "unknown_subcommand");
    assert_eq!(stderr["error"]["details"]["didYouMean"], "search");
    assert!(stderr["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("search"));
}

#[test]
fn clap_nonsense_subcommand_has_no_false_suggestion() {
    // Trust clap's similarity threshold: nonsense input gets NO suggestion rather
    // than a re-derived false match.
    for argv in [&["xyz"][..], &["q"][..]] {
        let output = run(argv);
        assert_eq!(output.status.code(), Some(1));
        let stderr = stderr_json(&output);
        assert_eq!(stderr["error"]["code"], "unknown_subcommand");
        assert!(
            stderr["error"]["details"]["didYouMean"].is_null(),
            "no false suggestion for {argv:?}, got {}",
            stderr["error"]["details"]
        );
    }
}

#[test]
fn clap_nested_subcommand_typo_suggests_most_similar_not_destructive() {
    // Regression: a `.next()` on clap's ascending-similarity list picked the LEAST
    // similar candidate, steering `websets event` toward the destructive `delete`.
    let output = run(&["websets", "event"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = stderr_json(&output);
    assert_eq!(stderr["error"]["details"]["didYouMean"], "events");
    assert_ne!(stderr["error"]["details"]["didYouMean"], "delete");
}

#[test]
fn placeholder_guard_allows_lowercase_real_id() {
    // Case-sensitive word prefixes: a real lowercase id is not a placeholder.
    let ok = run_ok_json(&[
        "websets",
        "get",
        "example_abc123",
        "--dry-run",
        "--print-request",
        "--compact",
    ]);
    assert_eq!(ok["data"]["request"]["path"], "/v0/websets/example_abc123");
}

#[test]
fn clap_unknown_flag_includes_did_you_mean() {
    let output = run(&["search", "--numresults", "5", "q"]);
    assert_eq!(output.status.code(), Some(1));
    let stderr = stderr_json(&output);
    assert_eq!(stderr["error"]["code"], "unknown_flag");
    assert!(stderr["error"]["details"]["didYouMean"]
        .as_str()
        .unwrap()
        .contains("--num-results"));
    assert!(stderr["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--num-results"));
}

#[test]
fn parse_contents_answer_context_similar() {
    assert_path(&["contents", "https://exa.ai/docs"], "contents");
    parses(&["contents", "--ids", "id1", "id2"]);
    assert_eq!(
        parse_err(&["contents"]).kind(),
        clap::error::ErrorKind::MissingRequiredArgument
    );
    assert_eq!(
        parse_err(&["contents", "https://exa.ai", "--ids", "id1"]).kind(),
        clap::error::ErrorKind::ArgumentConflict
    );
    assert_path(&["answer", "What is Exa?"], "answer");
    parses(&[
        "contents",
        "https://exa.ai/docs",
        "--text",
        "--summary-query",
        "Summarize the page",
    ]);
    parses(&[
        "answer",
        "What is Exa?",
        "--text",
        "--stream",
        "--output-schema",
        r#"{"type":"object"}"#,
    ]);
    assert_path(&["context", "rust async patterns"], "context");
    parses(&["context", "rust async patterns", "--tokens", "dynamic"]);
    parses(&["context", "rust async patterns", "--tokens", "1000"]);
    parses(&[
        "similar",
        "https://exa.ai",
        "--exclude-source-domain",
        "--category",
        "news",
    ]);
}

#[test]
fn parse_contents_requires_exactly_one_input_kind() {
    assert_eq!(
        parse_err(&["contents"]).kind(),
        clap::error::ErrorKind::MissingRequiredArgument
    );
    assert_eq!(
        parse_err(&["contents", "https://exa.ai", "--ids", "id1"]).kind(),
        clap::error::ErrorKind::ArgumentConflict
    );
}

#[test]
fn parse_contents_accepts_urls_only_and_ids_only() {
    let cli = parses(&[
        "contents",
        "https://exa.ai/docs",
        "https://docs.exa.ai/reference/search",
        "--text",
        "--summary-query",
        "Summarize the page",
        "--chunk-size",
        "100",
    ]);
    let Command::Contents(args) = cli.command else {
        panic!("expected contents command");
    };
    assert_eq!(
        args.urls,
        vec![
            "https://exa.ai/docs".to_string(),
            "https://docs.exa.ai/reference/search".to_string()
        ]
    );
    assert!(args.ids.is_empty());
    assert!(args.text);
    assert_eq!(args.summary_query.as_deref(), Some("Summarize the page"));
    assert_eq!(args.chunk_size, Some(100));

    let cli = parses(&["contents", "--ids", "doc_1", "doc_2"]);
    let Command::Contents(args) = cli.command else {
        panic!("expected contents command");
    };
    assert!(args.urls.is_empty());
    assert_eq!(args.ids, vec!["doc_1".to_string(), "doc_2".to_string()]);
    assert!(!args.text);
    assert_eq!(args.summary_query, None);
    assert_eq!(args.chunk_size, None);
}

#[test]
fn parse_contents_rejects_zero_chunk_size() {
    assert_eq!(
        parse_err(&["contents", "https://exa.ai", "--chunk-size", "0"]).kind(),
        clap::error::ErrorKind::ValueValidation
    );
    assert_eq!(
        parse_err(&["contents", "https://exa.ai", "--chunk-size", "101"]).kind(),
        clap::error::ErrorKind::ValueValidation
    );
}

#[test]
fn parse_team_info() {
    assert_path(&["team", "info"], "team info");
}

#[test]
fn parse_agent_runs_lifecycle() {
    assert_path(
        &["agent", "runs", "create", "find eval tools"],
        "agent runs create",
    );
    assert_path(&["agent", "run", "find eval tools"], "agent run");
    assert_path(&["agent", "runs", "list"], "agent runs list");
    assert_path(&["agent", "runs", "get", "agent_run_abc"], "agent runs get");
    parses(&["agent", "runs", "events", "agent_run_abc", "--stream"]);
    assert_path(
        &["agent", "runs", "cancel", "agent_run_abc"],
        "agent runs cancel",
    );
    assert_path(
        &["agent", "runs", "delete", "agent_run_abc"],
        "agent runs delete",
    );
}

#[test]
fn agent_runs_dry_run_builds_create_list_get_events_cancel_and_delete() {
    let create = run_ok_json(&[
        "agent",
        "runs",
        "create",
        "find eval tools",
        "--effort",
        "medium",
        "--stream",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(create["command"], "agent runs create");
    assert_eq!(create["data"]["request"]["path"], "/agent/runs");
    assert_eq!(
        create["data"]["request"]["body"]["query"],
        "find eval tools"
    );
    assert_eq!(create["data"]["request"]["body"]["effort"], "medium");
    assert!(create["data"]["request"]["body"].get("stream").is_none());
    assert_eq!(create["data"]["request"]["headers"][0]["name"], "Accept");
    assert_eq!(
        create["data"]["request"]["headers"][0]["value"],
        "text/event-stream"
    );

    let run_macro = run_ok_json(&["agent", "run", "macro query", "--dry-run", "--compact"]);
    assert_eq!(run_macro["command"], "agent run");
    assert_eq!(run_macro["data"]["request"]["path"], "/agent/runs");
    assert_eq!(run_macro["data"]["request"]["body"]["query"], "macro query");
    assert!(run_macro["data"]["expandsTo"]
        .as_str()
        .unwrap()
        .contains("agent runs create"));

    let list = run_ok_json(&[
        "agent",
        "runs",
        "list",
        "--limit",
        "5",
        "--cursor",
        "cur_agent",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(list["command"], "agent runs list");
    assert_eq!(list["data"]["request"]["path"], "/agent/runs");
    assert_eq!(
        list["data"]["request"]["query"],
        serde_json::json!([
            {"name": "limit", "value": "5"},
            {"name": "cursor", "value": "cur_agent"}
        ])
    );

    let get = run_ok_json(&[
        "agent",
        "runs",
        "get",
        "agent_run/abc",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(get["command"], "agent runs get");
    assert_eq!(
        get["data"]["request"]["path"],
        "/agent/runs/agent_run%2Fabc"
    );

    let events = run_ok_json(&[
        "agent",
        "runs",
        "events",
        "agent_run_abc",
        "--stream",
        "--last-event-id",
        "evt_1",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(events["command"], "agent runs events");
    assert_eq!(
        events["data"]["request"]["path"],
        "/agent/runs/agent_run_abc/events"
    );
    let headers = events["data"]["request"]["headers"]
        .as_array()
        .expect("stream preview headers");
    assert!(headers
        .iter()
        .any(|header| { header["name"] == "Accept" && header["value"] == "text/event-stream" }));
    assert!(headers
        .iter()
        .any(|header| { header["name"] == "Last-Event-ID" && header["value"] == "evt_1" }));

    let cancel = run_ok_json(&[
        "agent",
        "runs",
        "cancel",
        "agent_run_abc",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(cancel["command"], "agent runs cancel");
    assert_eq!(
        cancel["data"]["request"]["path"],
        "/agent/runs/agent_run_abc/cancel"
    );

    let delete = run_ok_json(&[
        "agent",
        "runs",
        "delete",
        "agent_run_abc",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(delete["command"], "agent runs delete");
    assert_eq!(
        delete["data"]["request"]["path"],
        "/agent/runs/agent_run_abc"
    );
}

#[test]
fn agent_runs_create_dry_run_builds_structured_create_fields() {
    let create = run_ok_json(&[
        "agent",
        "runs",
        "create",
        "enrich target accounts",
        "--output-schema",
        r#"{"type":"object","properties":{"name":{"type":"string"}}}"#,
        "--input",
        r#"{"exclusion":[{"domain":"old.example"}]}"#,
        "--input-row",
        r#"{"company":"OpenAI"}"#,
        "--input-row",
        r#"{"company":"Anthropic"}"#,
        "--exclusion",
        r#"[{"company":"Blocked"}]"#,
        "--previous-run-id",
        "agent_run_prev",
        "--data-source",
        "similarweb",
        "--data-source",
        "fiber_ai",
        "--metadata",
        r#"{"ticket":"T1","owner":"ops"}"#,
        "--dry-run",
        "--compact",
    ]);

    let body = &create["data"]["request"]["body"];
    assert_eq!(body["query"], "enrich target accounts");
    assert_eq!(
        body["outputSchema"],
        serde_json::json!({"type":"object","properties":{"name":{"type":"string"}}})
    );
    assert_eq!(
        body["input"]["data"],
        serde_json::json!([
            {"company":"OpenAI"},
            {"company":"Anthropic"}
        ])
    );
    assert_eq!(
        body["input"]["exclusion"],
        serde_json::json!([{"company":"Blocked"}])
    );
    assert_eq!(body["previousRunId"], "agent_run_prev");
    assert_eq!(
        body["dataSources"],
        serde_json::json!([
            {"provider":"similarweb"},
            {"provider":"fiber_ai"}
        ])
    );
    assert_eq!(
        body["metadata"],
        serde_json::json!({"ticket":"T1","owner":"ops"})
    );
}

#[test]
fn agent_runs_create_rejects_bad_structured_create_fields() {
    let input_row = run(&[
        "agent",
        "runs",
        "create",
        "enrich target accounts",
        "--input-row",
        r#"["not","object"]"#,
        "--compact",
    ]);
    assert_eq!(input_row.status.code(), Some(1));
    let stderr = stderr_json(&input_row);
    assert_eq!(stderr["error"]["code"], "invalid_value");
    assert!(stderr["error"]["message"]
        .as_str()
        .unwrap()
        .contains("JSON object"));

    let too_many_sources = run(&[
        "agent",
        "runs",
        "create",
        "enrich target accounts",
        "--data-source",
        "a",
        "--data-source",
        "b",
        "--data-source",
        "c",
        "--data-source",
        "d",
        "--data-source",
        "e",
        "--data-source",
        "f",
        "--compact",
    ]);
    assert_eq!(too_many_sources.status.code(), Some(1));
    let stderr = stderr_json(&too_many_sources);
    assert_eq!(stderr["error"]["code"], "invalid_value");
    assert!(stderr["error"]["message"]
        .as_str()
        .unwrap()
        .contains("at most 5"));

    let empty_source = run(&[
        "agent",
        "runs",
        "create",
        "enrich target accounts",
        "--data-source",
        "",
        "--compact",
    ]);
    assert_eq!(empty_source.status.code(), Some(1));
    let stderr = stderr_json(&empty_source);
    assert_eq!(stderr["error"]["code"], "invalid_value");
    assert!(stderr["error"]["message"]
        .as_str()
        .unwrap()
        .contains("must not be empty"));
}

#[cfg(unix)]
#[test]
fn sse_sigint_stalled_stream_exits_12_with_last_event_id() {
    const RUN_ID: &str = "run_sigint_test";
    let event_data = r#"{"type":"progress","message":"partial"}"#;
    let (base_url, stop_server, server) = local_sse_stall_server(RUN_ID, event_data);

    let mut child = command(&[
        "agent",
        "runs",
        "events",
        RUN_ID,
        "--stream",
        "--last-event-id",
        "evt-resume",
        "--ndjson",
        "--base-url",
        base_url.as_str(),
        "--api-key",
        "test-key-abcdef12",
        "--compact",
    ])
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .expect("spawn exa-agent SSE stream");

    let stdout = child.stdout.take().expect("stdout pipe");
    let mut reader = std::io::BufReader::new(stdout);
    let mut first_line = String::new();
    reader
        .read_line(&mut first_line)
        .expect("read first SSE NDJSON line from stdout");
    assert!(
        !first_line.trim().is_empty(),
        "expected one NDJSON event on stdout before SIGINT"
    );

    let event: serde_json::Value =
        serde_json::from_str(first_line.trim()).expect("first stdout line was not JSON");
    assert_eq!(event["schema"], "exa.cli.event.v1");
    assert_eq!(event["eventId"], "evt-42");
    assert_eq!(event["command"], "agent runs events");

    let pid = child.id();
    let kill_status = ProcessCommand::new("/bin/kill")
        .args(["-INT", &pid.to_string()])
        .status()
        .expect("send SIGINT to exa-agent child");
    assert!(kill_status.success(), "kill -INT failed for pid {pid}");

    let output = child
        .wait_with_output()
        .expect("wait for interrupted exa-agent child");
    let _ = stop_server.send(());
    server.join().expect("local SSE test server panicked");

    assert_eq!(
        output.status.code(),
        Some(12),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty() || output.stdout.ends_with(b"\n"));
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap_or_else(|e| {
        panic!(
            "stderr was not JSON: {e}\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert_eq!(stderr["schema"], "exa.cli.error.v1");
    assert_eq!(stderr["error"]["code"], "interrupted");
    assert_eq!(stderr["error"]["category"], "interrupted");
    assert_eq!(stderr["error"]["details"]["lastEventId"], "evt-42");
}

#[test]
fn golden_paginated_all_ndjson() {
    let (base_url, server) = local_paginated_agent_runs_server();
    let output = run_owned(&[
        "agent".into(),
        "runs".into(),
        "list".into(),
        "--all".into(),
        "--limit".into(),
        "1".into(),
        "--ndjson".into(),
        "--base-url".into(),
        base_url,
        "--api-key".into(),
        "test-key-abcdef12".into(),
        "--compact".into(),
    ]);
    server
        .join()
        .expect("local pagination test server panicked");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "stdout:\n{stdout}");
    let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    let second: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(first["schema"], "exa.cli.response.v1");
    assert_eq!(first["command"], "agent runs list");
    assert_eq!(first["data"]["data"][0]["id"], "agent_run_1");
    assert_eq!(first["pagination"]["cursor"], serde_json::Value::Null);
    assert_eq!(first["pagination"]["nextCursor"], "cur2");
    assert_eq!(first["pagination"]["hasMore"], true);
    assert_eq!(first["pagination"]["autoPaginated"], true);
    assert_eq!(first["pagination"]["page"], 1);
    assert_eq!(second["schema"], "exa.cli.response.v1");
    assert_eq!(second["command"], "agent runs list");
    assert_eq!(second["data"]["data"][0]["id"], "agent_run_2");
    assert_eq!(second["pagination"]["cursor"], "cur2");
    assert_eq!(second["pagination"]["nextCursor"], serde_json::Value::Null);
    assert_eq!(second["pagination"]["hasMore"], false);
    assert_eq!(second["pagination"]["autoPaginated"], true);
    assert_eq!(second["pagination"]["page"], 2);
}

#[test]
fn agent_runs_events_rejects_mixed_replay_and_pagination_modes() {
    let no_stream = run(&[
        "agent",
        "runs",
        "events",
        "agent_run_abc",
        "--last-event-id",
        "evt_1",
        "--compact",
    ]);
    assert_eq!(no_stream.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&no_stream.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_flag_combination");
    assert!(stderr["error"]["message"]
        .as_str()
        .unwrap()
        .contains("--last-event-id"));

    let stream_with_cursor = run(&[
        "agent",
        "runs",
        "events",
        "agent_run_abc",
        "--stream",
        "--cursor",
        "cur_agent",
        "--compact",
    ]);
    assert_eq!(stream_with_cursor.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&stream_with_cursor.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_flag_combination");
    assert!(stderr["error"]["message"]
        .as_str()
        .unwrap()
        .contains("cursor pagination"));
}

#[test]
fn every_destructive_op_refuses_without_confirmation() {
    let mut saw_websets_searches_cancel = false;

    for op in registry::REGISTRY.iter().filter(|op| op.destructive()) {
        let command = op.command();
        let args = destructive_refusal_args(&command)
            .unwrap_or_else(|| panic!("missing destructive refusal fixture for {command}"));
        let output = run(&args);
        let stderr = assert_confirmation_required(&output, &command);

        match op.confirm_protocol() {
            Some(ConfirmProtocol::Yes) => assert!(
                stderr["error"]["suggestedCommand"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("--yes"),
                "{command} should suggest --yes"
            ),
            Some(ConfirmProtocol::EchoId) => assert!(
                stderr["error"]["suggestedCommand"]
                    .as_str()
                    .unwrap_or_default()
                    .contains("--confirm"),
                "{command} should suggest --confirm"
            ),
            Some(ConfirmProtocol::YesPlusEcho(token)) => assert!(
                stderr["error"]["suggestedCommand"]
                    .as_str()
                    .unwrap_or_default()
                    .contains(token),
                "{command} should suggest --confirm {token}"
            ),
            None => panic!("{command} is destructive but has no Confirm capability"),
        }

        if command == "websets searches cancel" {
            saw_websets_searches_cancel = true;
            assert!(stderr["error"]["message"]
                .as_str()
                .unwrap_or_default()
                .contains("websets searches cancel"));
        }
    }

    assert!(
        saw_websets_searches_cancel,
        "websets searches cancel must be in the destructive refusal invariant"
    );

    let monitor_batch =
        registry::lookup_by_command("monitor batch").expect("monitor batch is in registry");
    assert_eq!(
        monitor_batch.confirm_protocol(),
        Some(ConfirmProtocol::YesPlusEcho("delete"))
    );
    let missing_yes = run(&[
        "monitor",
        "batch",
        "--body",
        r#"{"action":"pause","filter":{"status":"active"},"dry_run":false}"#,
        "--compact",
    ]);
    assert_confirmation_required(&missing_yes, "monitor batch");
    let missing_confirm = run(&[
        "monitor",
        "batch",
        "--body",
        r#"{"action":"delete","filter":{"name":"daily"},"dry_run":false}"#,
        "--yes",
        "--compact",
    ]);
    let stderr = assert_confirmation_required(&missing_confirm, "monitor batch");
    assert!(stderr["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .contains("--confirm delete"));
}

#[test]
fn agent_runs_delete_requires_yes_for_live_execution() {
    let output = run(&["agent", "runs", "delete", "agent_run_abc", "--compact"]);
    assert_eq!(output.status.code(), Some(9));
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "confirmation_required");
    assert!(stderr["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--yes"));

    let allowed = run(&[
        "agent",
        "runs",
        "delete",
        "agent_run_abc",
        "--yes",
        "--dry-run",
        "--compact",
    ]);
    assert!(
        allowed.status.success(),
        "delete with --yes should preview: {}",
        String::from_utf8_lossy(&allowed.stderr)
    );
}

#[test]
fn agent_runs_list_all_dry_run_previews_first_page() {
    let list_all = run_ok_json(&["agent", "runs", "list", "--all", "--dry-run", "--compact"]);
    assert_eq!(list_all["command"], "agent runs list");
    assert_eq!(list_all["data"]["request"]["path"], "/agent/runs");
    assert_eq!(list_all["data"]["request"]["query"], serde_json::json!([]));
}

#[test]
fn parse_research_commands() {
    assert_path(&["research", "create", "legacy query"], "research create");
    assert_path(&["research", "list"], "research list");
    assert_path(&["research", "get", "research_abc"], "research get");
}

#[test]
fn parse_monitor_commands() {
    assert_path(
        &["monitor", "create", "--name", "daily", "--query", "AI news"],
        "monitor create",
    );
    assert_path(&["monitor", "list"], "monitor list");
    assert_path(&["monitor", "get", "mon_abc"], "monitor get");
    assert_path(
        &["monitor", "update", "mon_abc", "--query", "new query"],
        "monitor update",
    );
    assert_path(&["monitor", "delete", "mon_abc"], "monitor delete");
    assert_path(&["monitor", "runs", "list", "mon_abc"], "monitor runs list");
}

#[test]
fn monitor_create_dry_run_builds_nested_body() {
    let create = run_ok_json(&[
        "monitor",
        "create",
        "--name",
        "daily",
        "--query",
        "AI news",
        "--schedule",
        "6h",
        "--webhook-url",
        "https://example.com/hook",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(create["command"], "monitor create");
    let body = &create["data"]["request"]["body"];
    assert_eq!(body["name"], "daily");
    assert_eq!(body["search"]["query"], "AI news");
    assert_eq!(body["trigger"]["type"], "interval");
    assert_eq!(body["trigger"]["period"], "6h");
    assert_eq!(body["webhook"]["url"], "https://example.com/hook");
    assert!(create["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["code"] == "webhook_secret_ephemeral"));

    let create_without_schedule = run_ok_json(&[
        "monitor",
        "create",
        "--query",
        "AI news",
        "--webhook-url",
        "https://example.com/hook",
        "--dry-run",
        "--compact",
    ]);
    assert!(
        create_without_schedule["data"]["request"]["body"]
            .get("trigger")
            .is_none(),
        "absent --schedule must not synthesize a trigger object"
    );
}

#[test]
fn monitor_create_body_set_precedence_over_named_flags() {
    let create = run_ok_json(&[
        "monitor",
        "create",
        "--query",
        "flag query",
        "--webhook-url",
        "https://example.com/flag",
        "--body",
        r#"{"search":{"query":"body query"},"webhook":{"url":"https://example.com/body"}}"#,
        "--set",
        "search.query=set query",
        "--dry-run",
        "--compact",
    ]);
    let body = &create["data"]["request"]["body"];
    assert_eq!(body["search"]["query"], "set query");
    assert_eq!(body["webhook"]["url"], "https://example.com/body");
}

#[test]
fn monitor_create_requires_search_query_and_webhook_url() {
    let output = run(&["monitor", "create", "--query", "AI news", "--compact"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "missing_required_argument");
    assert!(stderr["error"]["message"]
        .as_str()
        .unwrap()
        .contains("search.query and webhook.url"));
}

#[test]
fn monitor_list_dry_run_includes_filters_and_metadata_brackets() {
    let list = run_ok_json(&[
        "monitor",
        "list",
        "--status",
        "active",
        "--name",
        "daily",
        "--metadata",
        "owner=ops",
        "--metadata",
        "team=search",
        "--limit",
        "10",
        "--cursor",
        "cur_mon",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(list["command"], "monitor list");
    assert_eq!(
        list["data"]["request"]["query"],
        serde_json::json!([
            {"name": "status", "value": "active"},
            {"name": "name", "value": "daily"},
            {"name": "metadata[owner]", "value": "ops"},
            {"name": "metadata[team]", "value": "search"},
            {"name": "limit", "value": "10"},
            {"name": "cursor", "value": "cur_mon"}
        ])
    );
}

fn local_paginated_monitor_list_server() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let responses = [
            r#"{"data":[{"id":"mon_1"}],"hasMore":true,"nextCursor":"cur2"}"#,
            r#"{"data":[{"id":"mon_2"}],"hasMore":false,"nextCursor":null}"#,
        ];
        for (idx, response_body) in responses.iter().enumerate() {
            let started = Instant::now();
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(accepted) => break accepted,
                    Err(err)
                        if err.kind() == ErrorKind::WouldBlock
                            && started.elapsed() < Duration::from_secs(10) =>
                    {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(err) => panic!("failed to accept local pagination test request: {err}"),
                }
            };
            let request = String::from_utf8_lossy(&read_http_request(&mut stream)).into_owned();
            if idx == 0 {
                assert!(
                    request.starts_with(
                        "GET /monitors?status=active&name=daily&metadata%5Bowner%5D=ops&limit=1 "
                    ),
                    "unexpected first page request:\n{request}"
                );
            } else {
                assert!(
                    request.starts_with(
                        "GET /monitors?status=active&name=daily&metadata%5Bowner%5D=ops&limit=1&cursor=cur2 "
                    ),
                    "unexpected second page request:\n{request}"
                );
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            )
            .unwrap();
            stream.flush().unwrap();
        }
    });
    (format!("http://{addr}"), server)
}

#[test]
fn monitor_list_all_live_preserves_filters_across_pages() {
    let (base_url, server) = local_paginated_monitor_list_server();
    let output = run_owned(&[
        "monitor".into(),
        "list".into(),
        "--all".into(),
        "--limit".into(),
        "1".into(),
        "--status".into(),
        "active".into(),
        "--name".into(),
        "daily".into(),
        "--metadata".into(),
        "owner=ops".into(),
        "--ndjson".into(),
        "--base-url".into(),
        base_url,
        "--api-key".into(),
        "test-key-abcdef12".into(),
        "--compact".into(),
    ]);
    server
        .join()
        .expect("local pagination test server panicked");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn monitor_delete_requires_yes_for_live_execution() {
    let output = run(&["monitor", "delete", "mon_abc", "--compact"]);
    assert_eq!(output.status.code(), Some(9));
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "confirmation_required");
    assert!(stderr["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--yes"));
}

#[test]
fn monitor_get_update_and_trigger_dry_run_shapes() {
    let get = run_ok_json(&["monitor", "get", "mon/abc", "--dry-run", "--compact"]);
    assert_eq!(get["command"], "monitor get");
    assert_eq!(get["data"]["request"]["path"], "/monitors/mon%2Fabc");
    assert!(get["data"]["request"]["body"].is_null());

    let update = run_ok_json(&[
        "monitor",
        "update",
        "mon_abc",
        "--name",
        "renamed",
        "--query",
        "new query",
        "--schedule",
        "1d",
        "--status",
        "paused",
        "--webhook-url",
        "https://example.com/new-hook",
        "--set",
        "search.query=set query",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(update["command"], "monitor update");
    assert_eq!(update["data"]["request"]["path"], "/monitors/mon_abc");
    let body = &update["data"]["request"]["body"];
    assert_eq!(body["name"], "renamed");
    assert_eq!(body["search"]["query"], "set query");
    assert_eq!(body["trigger"]["type"], "interval");
    assert_eq!(body["trigger"]["period"], "1d");
    assert_eq!(body["status"], "paused");
    assert_eq!(body["webhook"]["url"], "https://example.com/new-hook");

    let empty_update = run(&["monitor", "update", "mon_abc", "--dry-run", "--compact"]);
    assert_eq!(empty_update.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&empty_update.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "missing_required_argument");
    assert!(stderr["error"]["message"]
        .as_str()
        .unwrap()
        .contains("requires at least one field"));

    let trigger = run_ok_json(&["monitor", "trigger", "mon_abc", "--dry-run", "--compact"]);
    assert_eq!(trigger["command"], "monitor trigger");
    assert_eq!(
        trigger["data"]["request"]["path"],
        "/monitors/mon_abc/trigger"
    );
    assert_eq!(trigger["data"]["request"]["body"], serde_json::json!({}));
}

#[test]
fn monitor_batch_defaults_dry_run_true() {
    let batch = run_ok_json(&[
        "monitor",
        "batch",
        "--body",
        r#"{"action":"pause","filter":{"status":"active"}}"#,
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(batch["command"], "monitor batch");
    assert_eq!(batch["data"]["request"]["body"]["dry_run"], true);
}

#[test]
fn monitor_batch_dry_run_false_requires_confirmation() {
    let missing_yes = run(&[
        "monitor",
        "batch",
        "--body",
        r#"{"action":"pause","filter":{"status":"active"},"dry_run":false}"#,
        "--compact",
    ]);
    assert_eq!(missing_yes.status.code(), Some(9));
    let stderr: serde_json::Value = serde_json::from_slice(&missing_yes.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "confirmation_required");

    let delete_missing_confirm = run(&[
        "monitor",
        "batch",
        "--body",
        r#"{"action":"delete","filter":{"name":"daily"},"dry_run":false}"#,
        "--yes",
        "--compact",
    ]);
    assert_eq!(delete_missing_confirm.status.code(), Some(9));
    let stderr: serde_json::Value = serde_json::from_slice(&delete_missing_confirm.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "confirmation_required");
    assert!(stderr["error"]["message"]
        .as_str()
        .unwrap()
        .contains("--confirm delete"));

    let non_boolean_dry_run = run(&[
        "monitor",
        "batch",
        "--body",
        r#"{"action":"pause","filter":{"status":"active"},"dry_run":"false"}"#,
        "--compact",
    ]);
    assert_eq!(non_boolean_dry_run.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&non_boolean_dry_run.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");
    assert!(stderr["error"]["message"]
        .as_str()
        .unwrap()
        .contains("must be a boolean"));
}

#[test]
fn monitor_runs_list_and_get_dry_run_paths() {
    let list = run_ok_json(&[
        "monitor",
        "runs",
        "list",
        "mon_abc",
        "--limit",
        "5",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(list["command"], "monitor runs list");
    assert_eq!(list["data"]["request"]["path"], "/monitors/mon_abc/runs");
    assert_eq!(
        list["data"]["request"]["query"],
        serde_json::json!([{"name": "limit", "value": "5"}])
    );

    let get = run_ok_json(&[
        "monitor",
        "runs",
        "get",
        "mon_abc",
        "run_xyz",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(get["command"], "monitor runs get");
    assert_eq!(
        get["data"]["request"]["path"],
        "/monitors/mon_abc/runs/run_xyz"
    );
}

#[test]
fn monitor_create_live_captures_webhook_secret_and_redacts_stdout() {
    let response = br#"{"id":"mon_test","webhookSecret":"whsec_live_capture_12345"}"#;
    let (base_url, server) = local_json_server(
        |request| {
            assert!(request.starts_with("POST /monitors "));
            assert!(request.contains(r#""query":"secret test""#));
        },
        response,
    );
    let secret_path = temp_path("webhook-secret").join("secret.txt");
    let output = run_owned(&[
        "monitor".into(),
        "create".into(),
        "--query".into(),
        "secret test".into(),
        "--webhook-url".into(),
        "https://example.com/hook".into(),
        "--secret-output".into(),
        secret_path.to_string_lossy().into_owned(),
        "--base-url".into(),
        base_url,
        "--api-key".into(),
        "test-key-abcdef12".into(),
        "--compact".into(),
    ]);
    server.join().expect("local monitor create server panicked");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    assert!(!stdout.contains("whsec_live_capture_12345"));
    assert!(stdout.contains("<redacted>"));
    let secret = fs::read_to_string(&secret_path).expect("secret file");
    assert_eq!(secret, "whsec_live_capture_12345");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&secret_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}

#[test]
fn monitor_create_secret_output_requires_webhook_secret() {
    let (base_url, server) = local_json_server(
        |request| {
            assert!(request.starts_with("POST /monitors "));
        },
        br#"{"id":"mon_test"}"#,
    );
    let secret_path = temp_path("webhook-secret-missing").join("secret.txt");
    let output = run_owned(&[
        "monitor".into(),
        "create".into(),
        "--query".into(),
        "secret test".into(),
        "--webhook-url".into(),
        "https://example.com/hook".into(),
        "--secret-output".into(),
        secret_path.to_string_lossy().into_owned(),
        "--base-url".into(),
        base_url,
        "--api-key".into(),
        "test-key-abcdef12".into(),
        "--compact".into(),
    ]);
    server.join().expect("local monitor create server panicked");
    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty());
    assert!(
        !secret_path.exists(),
        "reserved secret file should be removed when the response omits webhookSecret"
    );
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "upstream_malformed");
    assert!(stderr["error"]["message"]
        .as_str()
        .unwrap()
        .contains("monitor create response did not include string"));
}

#[test]
fn monitor_create_bad_secret_output_fails_before_post() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let missing_parent = temp_path("webhook-secret-preflight")
        .join("missing")
        .join("secret.txt");
    let output = run_owned(&[
        "monitor".into(),
        "create".into(),
        "--query".into(),
        "secret test".into(),
        "--webhook-url".into(),
        "https://example.com/hook".into(),
        "--secret-output".into(),
        missing_parent.to_string_lossy().into_owned(),
        "--base-url".into(),
        base_url,
        "--api-key".into(),
        "test-key-abcdef12".into(),
        "--compact".into(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");
    assert!(stderr["error"]["message"]
        .as_str()
        .unwrap()
        .contains("before request"));
    match listener.accept() {
        Err(err) if err.kind() == ErrorKind::WouldBlock => {}
        Ok(_) => panic!("monitor create sent a request before secret-output preflight succeeded"),
        Err(err) => panic!("unexpected listener error: {err}"),
    }
}

#[test]
fn monitor_create_existing_secret_output_fails_before_post() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let secret_path = temp_path("webhook-secret-existing").join("secret.txt");
    fs::write(&secret_path, "do not overwrite").unwrap();
    let output = run_owned(&[
        "monitor".into(),
        "create".into(),
        "--query".into(),
        "secret test".into(),
        "--webhook-url".into(),
        "https://example.com/hook".into(),
        "--secret-output".into(),
        secret_path.to_string_lossy().into_owned(),
        "--base-url".into(),
        base_url,
        "--api-key".into(),
        "test-key-abcdef12".into(),
        "--compact".into(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        fs::read_to_string(&secret_path).unwrap(),
        "do not overwrite"
    );
    match listener.accept() {
        Err(err) if err.kind() == ErrorKind::WouldBlock => {}
        Ok(_) => panic!("monitor create sent a request before existing secret-output failed"),
        Err(err) => panic!("unexpected listener error: {err}"),
    }
}

#[test]
fn monitor_create_secret_output_refuses_stdout() {
    let output = run(&[
        "monitor",
        "create",
        "--query",
        "AI news",
        "--webhook-url",
        "https://example.com/hook",
        "--secret-output",
        "-",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");
}

#[test]
fn monitor_create_raw_is_rejected_for_secret_safety() {
    let output = run(&[
        "monitor",
        "create",
        "--query",
        "AI news",
        "--webhook-url",
        "https://example.com/hook",
        "--raw",
        "--compact",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_flag_combination");
}

fn assert_pending_record(path: &std::path::Path, command: &str, api_path: &str) {
    let raw = fs::read_to_string(path).expect("pending run file");
    let lines: Vec<_> = raw.lines().collect();
    assert_eq!(lines.len(), 1);
    let record: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(record["schema"], "exa.cli.pending_run.v1");
    assert_eq!(record["command"], command);
    assert_eq!(record["apiPath"], api_path);
    assert!(record["requestId"]
        .as_str()
        .is_some_and(|request_id| request_id.starts_with("req_")));
    assert_eq!(
        record["recoveryCommand"],
        format!("exa-agent {command} --idempotency-key <stable-key>")
    );
}

#[test]
fn monitor_create_ambiguous_failure_records_pending_run() {
    let dir = temp_path("monitor-create-pending");
    let pending_path = dir.join("pending-runs.jsonl");
    let pending_path_string = pending_path.to_string_lossy().into_owned();
    let base_url = closed_local_base_url();
    let output = run_with_env(
        &[
            "monitor",
            "create",
            "--query",
            "pending recovery",
            "--webhook-url",
            "https://example.com/hook",
            "--base-url",
            base_url.as_str(),
            "--api-key",
            "test-key-abcdef12",
            "--compact",
        ],
        &[("EXA_AGENT_PENDING_RUNS", pending_path_string.as_str())],
    );
    assert!(!output.status.success());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["details"]["pendingRunWritten"], true);
    assert_pending_record(&pending_path, "monitor create", "/monitors");
}

#[test]
fn parse_websets_representative_nested() {
    assert_path(
        &[
            "websets",
            "create",
            "--query",
            "SF startups",
            "--count",
            "10",
        ],
        "websets create",
    );
    assert_path(&["websets", "list"], "websets list");
    assert_path(&["websets", "get", "webset_abc"], "websets get");
    assert_path(
        &["websets", "items", "list", "webset_abc"],
        "websets items list",
    );
    assert_path(
        &[
            "websets",
            "searches",
            "create",
            "webset_abc",
            "--query",
            "founders",
        ],
        "websets searches create",
    );
    assert_path(
        &["websets", "enrichments", "get", "webset_abc", "enr_abc"],
        "websets enrichments get",
    );
    assert_path(&["websets", "imports", "list"], "websets imports list");
    assert_path(&["websets", "monitors", "list"], "websets monitors list");
    assert_path(&["websets", "events", "list"], "websets events list");
    assert_path(
        &["websets", "webhooks", "attempts", "list", "wh_abc"],
        "websets webhooks attempts list",
    );
}

#[test]
fn websets_create_and_preview_dry_run_build_nested_body_and_precedence() {
    let create = run_ok_json(&[
        "websets",
        "create",
        "--query",
        "SF startups",
        "--count",
        "10",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(create["command"], "websets create");
    assert_eq!(create["data"]["request"]["path"], "/v0/websets");
    let body = &create["data"]["request"]["body"];
    assert_eq!(body["search"]["query"], "SF startups");
    assert_eq!(body["search"]["count"], 10);

    let default_count = run_ok_json(&[
        "websets",
        "create",
        "--query",
        "SF startups",
        "--dry-run",
        "--compact",
    ]);
    let body = &default_count["data"]["request"]["body"];
    assert_eq!(body["search"]["query"], "SF startups");
    assert!(body["search"].get("count").is_none());

    let precedence = run_ok_json(&[
        "websets",
        "create",
        "--query",
        "flag query",
        "--count",
        "5",
        "--body",
        r#"{"search":{"query":"body query","count":7}}"#,
        "--set",
        "search.count=12",
        "--dry-run",
        "--compact",
    ]);
    let body = &precedence["data"]["request"]["body"];
    assert_eq!(body["search"]["query"], "body query");
    assert_eq!(body["search"]["count"], 12);

    let missing_body = run(&["websets", "create", "--compact"]);
    assert_eq!(missing_body.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&missing_body.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "missing_required_argument");

    let zero_create_count = run(&[
        "websets",
        "create",
        "--query",
        "SF startups",
        "--count",
        "0",
        "--compact",
    ]);
    assert_eq!(zero_create_count.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&zero_create_count.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");

    let preview = run_ok_json(&[
        "websets",
        "preview",
        "--query",
        "AI tools",
        "--count",
        "3",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(preview["command"], "websets preview");
    assert_eq!(preview["data"]["request"]["path"], "/v0/websets/preview");
    let preview_body = &preview["data"]["request"]["body"];
    assert_eq!(preview_body["search"]["query"], "AI tools");
    assert_eq!(preview_body["search"]["count"], 3);
    assert!(preview_body["search"].get("criteria").is_none());
    assert_eq!(
        preview["data"]["request"]["query"],
        serde_json::json!([{"name": "search", "value": "true"}])
    );

    let decomposition_only = run_ok_json(&[
        "websets",
        "preview",
        "--query",
        "AI tools",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(
        decomposition_only["data"]["request"]["query"],
        serde_json::json!([])
    );

    let preview_missing_query = run(&["websets", "preview", "--count", "3", "--compact"]);
    assert_eq!(preview_missing_query.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&preview_missing_query.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "missing_required_argument");

    let preview_count_too_high = run(&[
        "websets",
        "preview",
        "--query",
        "AI tools",
        "--count",
        "11",
        "--compact",
    ]);
    assert_eq!(preview_count_too_high.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&preview_count_too_high.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");

    let preview_criteria = run(&[
        "websets",
        "preview",
        "--query",
        "AI tools",
        "--criteria",
        "must be B2B",
        "--compact",
    ]);
    assert_eq!(preview_criteria.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&preview_criteria.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");
}

#[test]
fn websets_create_missing_fields_uses_usage_exit_code() {
    let output = run(&["websets", "create", "--compact"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr = stderr_json(&output);
    assert_eq!(stderr["error"]["code"], "missing_required_argument");
    assert_eq!(stderr["error"]["category"], "usage");
    assert_eq!(stderr["error"]["exitCode"], 1);
}

#[test]
fn websets_get_rejects_placeholder_ids_before_auth() {
    for placeholder in ["<id>", "$RUN_ID", "YOUR_WEBSET_ID"] {
        let output = run(&["websets", "get", placeholder, "--compact"]);
        assert_eq!(output.status.code(), Some(1), "placeholder {placeholder}");
        assert!(output.stdout.is_empty());
        let stderr = stderr_json(&output);
        assert_eq!(stderr["error"]["code"], "placeholder_argument");
        assert_eq!(stderr["error"]["category"], "usage");
        assert!(stderr["error"]["message"]
            .as_str()
            .unwrap()
            .contains(placeholder));
        assert!(stderr["error"]["suggestedCommand"]
            .as_str()
            .unwrap()
            .contains("websets get webset_123"));
    }

    let ok = run_ok_json(&["websets", "get", "webset_123", "--dry-run", "--compact"]);
    assert_eq!(ok["command"], "websets get");
    assert_eq!(ok["data"]["request"]["path"], "/v0/websets/webset_123");
}

fn local_paginated_websets_list_server() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let responses = [
            r#"{"data":[{"id":"ws_1"}],"hasMore":true,"nextCursor":"cur2"}"#,
            r#"{"data":[{"id":"ws_2"}],"hasMore":false,"nextCursor":null}"#,
        ];
        for (idx, response_body) in responses.iter().enumerate() {
            let started = Instant::now();
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(accepted) => break accepted,
                    Err(err)
                        if err.kind() == ErrorKind::WouldBlock
                            && started.elapsed() < Duration::from_secs(10) =>
                    {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(err) => panic!("failed to accept local pagination test request: {err}"),
                }
            };
            let request = String::from_utf8_lossy(&read_http_request(&mut stream)).into_owned();
            if idx == 0 {
                assert!(
                    request.starts_with("GET /v0/websets?search=founders&limit=1 "),
                    "unexpected first page request:\n{request}"
                );
            } else {
                assert!(
                    request.starts_with("GET /v0/websets?search=founders&limit=1&cursor=cur2 "),
                    "unexpected second page request:\n{request}"
                );
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            )
            .unwrap();
            stream.flush().unwrap();
        }
    });
    (format!("http://{addr}"), server)
}

#[test]
fn websets_list_all_preserves_search_filter_across_pages() {
    let (base_url, server) = local_paginated_websets_list_server();
    let output = run_owned(&[
        "websets".into(),
        "list".into(),
        "--all".into(),
        "--limit".into(),
        "1".into(),
        "--search".into(),
        "founders".into(),
        "--ndjson".into(),
        "--base-url".into(),
        base_url,
        "--api-key".into(),
        "test-key-abcdef12".into(),
        "--compact".into(),
    ]);
    server
        .join()
        .expect("local websets pagination test server panicked");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn local_paginated_websets_items_server() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let responses = [
            r#"{"data":[{"id":"item_1"}],"hasMore":true,"nextCursor":"cur2"}"#,
            r#"{"data":[{"id":"item_2"}],"hasMore":false,"nextCursor":null}"#,
        ];
        for (idx, response_body) in responses.iter().enumerate() {
            let started = Instant::now();
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(accepted) => break accepted,
                    Err(err)
                        if err.kind() == ErrorKind::WouldBlock
                            && started.elapsed() < Duration::from_secs(10) =>
                    {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(err) => {
                        panic!("failed to accept local items pagination test request: {err}")
                    }
                }
            };
            let request = String::from_utf8_lossy(&read_http_request(&mut stream)).into_owned();
            if idx == 0 {
                assert!(
                    request.starts_with("GET /v0/websets/ws_abc/items?sourceId=src_1&limit=1 "),
                    "unexpected first page request:\n{request}"
                );
            } else {
                assert!(
                    request.starts_with(
                        "GET /v0/websets/ws_abc/items?sourceId=src_1&limit=1&cursor=cur2 "
                    ),
                    "unexpected second page request:\n{request}"
                );
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            )
            .unwrap();
            stream.flush().unwrap();
        }
    });
    (format!("http://{addr}"), server)
}

#[test]
fn websets_items_list_all_preserves_source_id_filter_across_pages() {
    let (base_url, server) = local_paginated_websets_items_server();
    let output = run_owned(&[
        "websets".into(),
        "items".into(),
        "list".into(),
        "ws_abc".into(),
        "--all".into(),
        "--limit".into(),
        "1".into(),
        "--source-id".into(),
        "src_1".into(),
        "--ndjson".into(),
        "--base-url".into(),
        base_url,
        "--api-key".into(),
        "test-key-abcdef12".into(),
        "--compact".into(),
    ]);
    server
        .join()
        .expect("local websets items pagination test server panicked");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn websets_get_update_delete_cancel_dry_run_and_safety_shapes() {
    let get = run_ok_json(&["websets", "get", "ws/abc", "--dry-run", "--compact"]);
    assert_eq!(get["command"], "websets get");
    assert_eq!(get["data"]["request"]["path"], "/v0/websets/ws%2Fabc");
    assert!(get["data"]["request"]["body"].is_null());

    let update = run_ok_json(&[
        "websets",
        "update",
        "ws_abc",
        "--set",
        "title=Renamed",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(update["command"], "websets update");
    assert_eq!(update["data"]["request"]["path"], "/v0/websets/ws_abc");
    assert_eq!(update["data"]["request"]["body"]["title"], "Renamed");

    let empty_update = run(&["websets", "update", "ws_abc", "--dry-run", "--compact"]);
    assert_eq!(empty_update.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&empty_update.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "missing_required_argument");

    let delete_preview = run_ok_json(&["websets", "delete", "ws_abc", "--dry-run", "--compact"]);
    assert_eq!(delete_preview["command"], "websets delete");
    assert_eq!(
        delete_preview["data"]["request"]["path"],
        "/v0/websets/ws_abc"
    );
    assert!(delete_preview["data"]["request"]["body"].is_null());

    let delete_live = run(&["websets", "delete", "ws_abc", "--compact"]);
    assert_eq!(delete_live.status.code(), Some(9));

    let cancel_preview = run_ok_json(&["websets", "cancel", "ws_abc", "--dry-run", "--compact"]);
    assert_eq!(cancel_preview["command"], "websets cancel");
    assert_eq!(
        cancel_preview["data"]["request"]["path"],
        "/v0/websets/ws_abc/cancel"
    );

    let cancel_live = run(&["websets", "cancel", "ws_abc", "--compact"]);
    assert_eq!(cancel_live.status.code(), Some(9));
}

#[test]
fn websets_items_paths_filters_and_delete_safety() {
    let list = run_ok_json(&[
        "websets",
        "items",
        "list",
        "ws_abc",
        "--source-id",
        "src_1",
        "--limit",
        "5",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(list["command"], "websets items list");
    assert_eq!(list["data"]["request"]["path"], "/v0/websets/ws_abc/items");
    assert_eq!(
        list["data"]["request"]["query"],
        serde_json::json!([
            {"name": "sourceId", "value": "src_1"},
            {"name": "limit", "value": "5"}
        ])
    );

    let get = run_ok_json(&[
        "websets",
        "items",
        "get",
        "ws_abc",
        "item/1",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(
        get["data"]["request"]["path"],
        "/v0/websets/ws_abc/items/item%2F1"
    );
    assert!(get["data"]["request"]["body"].is_null());

    let delete_preview = run_ok_json(&[
        "websets",
        "items",
        "delete",
        "ws_abc",
        "item_1",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(delete_preview["command"], "websets items delete");
    assert_eq!(
        delete_preview["data"]["request"]["path"],
        "/v0/websets/ws_abc/items/item_1"
    );

    let delete_live = run(&[
        "websets",
        "items",
        "delete",
        "ws_abc",
        "item_1",
        "--compact",
    ]);
    assert_eq!(delete_live.status.code(), Some(9));
}

#[test]
fn websets_searches_create_get_cancel_shapes() {
    let create = run_ok_json(&[
        "websets",
        "searches",
        "create",
        "ws_abc",
        "--query",
        "founders",
        "--count",
        "25",
        "--criteria",
        "must be technical",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(create["command"], "websets searches create");
    assert_eq!(
        create["data"]["request"]["path"],
        "/v0/websets/ws_abc/searches"
    );
    let body = &create["data"]["request"]["body"];
    assert_eq!(body["query"], "founders");
    assert_eq!(body["count"], 25);
    assert_eq!(
        body["criteria"],
        serde_json::json!([{"description": "must be technical"}])
    );

    let missing = run(&[
        "websets",
        "searches",
        "create",
        "ws_abc",
        "--query",
        "founders",
        "--compact",
    ]);
    assert_eq!(missing.status.code(), Some(1));

    let zero_count = run(&[
        "websets",
        "searches",
        "create",
        "ws_abc",
        "--query",
        "founders",
        "--count",
        "0",
        "--compact",
    ]);
    assert_eq!(zero_count.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&zero_count.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");

    let get = run_ok_json(&[
        "websets",
        "searches",
        "get",
        "ws_abc",
        "search_1",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(
        get["data"]["request"]["path"],
        "/v0/websets/ws_abc/searches/search_1"
    );

    let cancel = run_ok_json(&[
        "websets",
        "searches",
        "cancel",
        "ws_abc",
        "search_1",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(cancel["command"], "websets searches cancel");
    assert_eq!(
        cancel["data"]["request"]["path"],
        "/v0/websets/ws_abc/searches/search_1/cancel"
    );

    let cancel_live = run(&[
        "websets",
        "searches",
        "cancel",
        "ws_abc",
        "search_1",
        "--compact",
    ]);
    let stderr = assert_confirmation_required(&cancel_live, "websets searches cancel");
    assert!(stderr["error"]["suggestedCommand"]
        .as_str()
        .unwrap_or_default()
        .contains("--yes"));
}

#[test]
fn websets_enrichments_create_update_delete_cancel_shapes() {
    let create = run_ok_json(&[
        "websets",
        "enrichments",
        "create",
        "ws_abc",
        "--description",
        "Company size",
        "--enrichment-format",
        "text",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(create["command"], "websets enrichments create");
    assert_eq!(
        create["data"]["request"]["path"],
        "/v0/websets/ws_abc/enrichments"
    );
    let body = &create["data"]["request"]["body"];
    assert_eq!(body["description"], "Company size");
    assert_eq!(body["format"], "text");

    let update = run_ok_json(&[
        "websets",
        "enrichments",
        "update",
        "ws_abc",
        "enr_1",
        "--description",
        "Updated label",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(update["command"], "websets enrichments update");
    assert_eq!(
        update["data"]["request"]["path"],
        "/v0/websets/ws_abc/enrichments/enr_1"
    );
    assert_eq!(
        update["data"]["request"]["body"]["description"],
        "Updated label"
    );

    let delete_live = run(&[
        "websets",
        "enrichments",
        "delete",
        "ws_abc",
        "enr_1",
        "--compact",
    ]);
    assert_eq!(delete_live.status.code(), Some(9));

    let cancel_live = run(&[
        "websets",
        "enrichments",
        "cancel",
        "ws_abc",
        "enr_1",
        "--compact",
    ]);
    assert_eq!(cancel_live.status.code(), Some(9));
}

#[test]
fn websets_imports_body_first_create_list_get_update_delete_shapes() {
    let create = run_ok_json(&[
        "websets",
        "imports",
        "create",
        "--source",
        "csv",
        "--body",
        r#"{"size":1024,"count":10,"entity":{"type":"company"}}"#,
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(create["command"], "websets imports create");
    assert_eq!(create["data"]["request"]["path"], "/v0/imports");
    let body = &create["data"]["request"]["body"];
    assert_eq!(body["format"], "csv");
    assert_eq!(body["size"], 1024);
    assert_eq!(body["count"], 10);

    let missing_body = run(&[
        "websets",
        "imports",
        "create",
        "--source",
        "csv",
        "--compact",
    ]);
    assert_eq!(missing_body.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&missing_body.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "missing_required_argument");

    let source_err = run(&[
        "websets",
        "imports",
        "create",
        "--source",
        "xml",
        "--body",
        r#"{"size":1024,"count":10,"entity":{"type":"company"}}"#,
        "--compact",
    ]);
    assert_eq!(source_err.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&source_err.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");

    let url_err = run(&[
        "websets",
        "imports",
        "create",
        "--url",
        "https://example.com/data.csv",
        "--compact",
    ]);
    assert_eq!(url_err.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&url_err.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");

    let csv_err = run(&[
        "websets",
        "imports",
        "create",
        "--csv",
        "/tmp/data.csv",
        "--compact",
    ]);
    assert_eq!(csv_err.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&csv_err.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "not_implemented");

    let list = run_ok_json(&[
        "websets",
        "imports",
        "list",
        "--limit",
        "10",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(list["command"], "websets imports list");
    assert_eq!(list["data"]["request"]["path"], "/v0/imports");

    let get = run_ok_json(&[
        "websets",
        "imports",
        "get",
        "imp_abc",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(get["data"]["request"]["path"], "/v0/imports/imp_abc");

    let update = run_ok_json(&[
        "websets",
        "imports",
        "update",
        "imp_abc",
        "--set",
        "title=Completed import",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(
        update["data"]["request"]["body"]["title"],
        "Completed import"
    );

    let delete_live = run(&["websets", "imports", "delete", "imp_abc", "--compact"]);
    assert_eq!(delete_live.status.code(), Some(9));
}

#[test]
fn websets_monitors_remain_distinct_from_top_level_monitor() {
    assert_path(&["monitor", "list"], "monitor list");
    assert_path(&["websets", "monitors", "list"], "websets monitors list");

    let top_level = run_ok_json(&["monitor", "list", "--dry-run", "--compact"]);
    assert_eq!(top_level["data"]["request"]["path"], "/monitors");

    let websets_monitors = run_ok_json(&["websets", "monitors", "list", "--dry-run", "--compact"]);
    assert_eq!(websets_monitors["command"], "websets monitors list");
    assert_eq!(websets_monitors["data"]["request"]["path"], "/v0/monitors");
}

#[test]
fn websets_monitors_create_update_dry_run_and_validation() {
    let create = run_ok_json(&[
        "websets",
        "monitors",
        "create",
        "--webset-id",
        "ws_abc",
        "--cron",
        "0 9 * * 1",
        "--timezone",
        "America/New_York",
        "--query",
        "new companies",
        "--count",
        "10",
        "--criteria",
        "must be technical",
        "--search-behavior",
        "append",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(create["command"], "websets monitors create");
    assert_eq!(create["data"]["request"]["path"], "/v0/monitors");
    let body = &create["data"]["request"]["body"];
    assert_eq!(body["websetId"], "ws_abc");
    assert_eq!(body["cadence"]["cron"], "0 9 * * 1");
    assert_eq!(body["cadence"]["timezone"], "America/New_York");
    assert_eq!(body["behavior"]["type"], "search");
    assert_eq!(body["behavior"]["config"]["count"], 10);
    assert_eq!(body["behavior"]["config"]["query"], "new companies");
    assert_eq!(body["behavior"]["config"]["behavior"], "append");
    assert_eq!(
        body["behavior"]["config"]["criteria"][0]["description"],
        "must be technical"
    );

    let missing = run(&[
        "websets",
        "monitors",
        "create",
        "--webset-id",
        "ws_abc",
        "--cron",
        "0 9 * * 1",
        "--compact",
    ]);
    assert_eq!(missing.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&missing.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "missing_required_argument");

    let bad_count = run(&[
        "websets",
        "monitors",
        "create",
        "--webset-id",
        "ws_abc",
        "--cron",
        "0 9 * * 1",
        "--count",
        "0",
        "--compact",
    ]);
    assert_eq!(bad_count.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&bad_count.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");

    let body_missing_behavior_type = run(&[
        "websets",
        "monitors",
        "create",
        "--body",
        r#"{"websetId":"ws_abc","cadence":{"cron":"0 9 * * 1"},"behavior":{"config":{"count":1}}}"#,
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(body_missing_behavior_type.status.code(), Some(1));
    let stderr: serde_json::Value =
        serde_json::from_slice(&body_missing_behavior_type.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "missing_required_argument");

    let body_empty_behavior_type = run(&[
        "websets",
        "monitors",
        "create",
        "--body",
        r#"{"websetId":"ws_abc","cadence":{"cron":"0 9 * * 1"},"behavior":{"type":"","config":{"count":1}}}"#,
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(body_empty_behavior_type.status.code(), Some(1));
    let stderr: serde_json::Value =
        serde_json::from_slice(&body_empty_behavior_type.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "missing_required_argument");

    let update = run_ok_json(&[
        "websets",
        "monitors",
        "update",
        "mon_abc",
        "--status",
        "disabled",
        "--cron",
        "0 14 * * *",
        "--count",
        "5",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(update["command"], "websets monitors update");
    assert_eq!(update["data"]["request"]["path"], "/v0/monitors/mon_abc");
    let body = &update["data"]["request"]["body"];
    assert_eq!(body["status"], "disabled");
    assert_eq!(body["cadence"]["cron"], "0 14 * * *");
    assert_eq!(body["behavior"]["config"]["count"], 5);

    let empty_update = run(&["websets", "monitors", "update", "mon_abc", "--compact"]);
    assert_eq!(empty_update.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&empty_update.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "missing_required_argument");

    let behavior_without_count = run(&[
        "websets",
        "monitors",
        "update",
        "mon_abc",
        "--query",
        "fresh companies",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(behavior_without_count.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&behavior_without_count.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");

    let cadence_without_cron = run(&[
        "websets",
        "monitors",
        "update",
        "mon_abc",
        "--timezone",
        "America/New_York",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(cadence_without_cron.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&cadence_without_cron.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");

    let delete_live = run(&["websets", "monitors", "delete", "mon_abc", "--compact"]);
    assert_eq!(delete_live.status.code(), Some(9));

    let runs_list = run_ok_json(&[
        "websets",
        "monitors",
        "runs",
        "list",
        "mon_abc",
        "--limit",
        "5",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(runs_list["command"], "websets monitors runs list");
    assert_eq!(
        runs_list["data"]["request"]["path"],
        "/v0/monitors/mon_abc/runs"
    );
    assert_eq!(
        runs_list["data"]["request"]["query"],
        serde_json::json!([{"name": "limit", "value": "5"}])
    );

    let runs_get = run_ok_json(&[
        "websets",
        "monitors",
        "runs",
        "get",
        "mon_abc",
        "run_xyz",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(
        runs_get["data"]["request"]["path"],
        "/v0/monitors/mon_abc/runs/run_xyz"
    );
}

#[test]
fn websets_events_list_filters_dry_run() {
    let list = run_ok_json(&[
        "websets",
        "events",
        "list",
        "--limit",
        "10",
        "--type",
        "webset.created",
        "--type",
        "monitor.created",
        "--created-before",
        "2026-06-01T00:00:00Z",
        "--created-after",
        "2026-05-01T00:00:00Z",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(list["command"], "websets events list");
    assert_eq!(list["data"]["request"]["path"], "/v0/events");
    let query = list["data"]["request"]["query"].as_array().unwrap();
    assert!(query
        .iter()
        .any(|entry| { entry["name"] == "types" && entry["value"] == "webset.created" }));
    assert!(query
        .iter()
        .any(|entry| { entry["name"] == "types" && entry["value"] == "monitor.created" }));
    assert!(query.iter().any(|entry| {
        entry["name"] == "createdBefore" && entry["value"] == "2026-06-01T00:00:00Z"
    }));
    assert!(query.iter().any(|entry| {
        entry["name"] == "createdAfter" && entry["value"] == "2026-05-01T00:00:00Z"
    }));
    assert!(query
        .iter()
        .any(|entry| entry["name"] == "limit" && entry["value"] == "10"));

    let get = run_ok_json(&[
        "websets",
        "events",
        "get",
        "evt_abc",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(get["data"]["request"]["path"], "/v0/events/evt_abc");
}

fn local_paginated_websets_events_server() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let responses = [
            r#"{"data":[{"id":"evt_1"}],"hasMore":true,"nextCursor":"cur2"}"#,
            r#"{"data":[{"id":"evt_2"}],"hasMore":false,"nextCursor":null}"#,
        ];
        for (idx, response_body) in responses.iter().enumerate() {
            let started = Instant::now();
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(accepted) => break accepted,
                    Err(err)
                        if err.kind() == ErrorKind::WouldBlock
                            && started.elapsed() < Duration::from_secs(10) =>
                    {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(err) => {
                        panic!("failed to accept local events pagination test request: {err}")
                    }
                }
            };
            let request = String::from_utf8_lossy(&read_http_request(&mut stream)).into_owned();
            if idx == 0 {
                assert!(
                    request.starts_with(
                        "GET /v0/events?types=webset.created&createdAfter=2026-01-01&limit=1 "
                    ),
                    "unexpected first page request:\n{request}"
                );
            } else {
                assert!(
                    request.starts_with(
                        "GET /v0/events?types=webset.created&createdAfter=2026-01-01&limit=1&cursor=cur2 "
                    ),
                    "unexpected second page request:\n{request}"
                );
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            )
            .unwrap();
            stream.flush().unwrap();
        }
    });
    (format!("http://{addr}"), server)
}

#[test]
fn websets_events_list_all_preserves_filters_across_pages() {
    let (base_url, server) = local_paginated_websets_events_server();
    let output = run_owned(&[
        "websets".into(),
        "events".into(),
        "list".into(),
        "--all".into(),
        "--limit".into(),
        "1".into(),
        "--type".into(),
        "webset.created".into(),
        "--created-after".into(),
        "2026-01-01".into(),
        "--ndjson".into(),
        "--base-url".into(),
        base_url,
        "--api-key".into(),
        "test-key-abcdef12".into(),
        "--compact".into(),
    ]);
    server
        .join()
        .expect("local websets events pagination test server panicked");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn local_paginated_websets_monitors_list_server() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let responses = [
            r#"{"data":[{"id":"mon_1"}],"hasMore":true,"nextCursor":"cur2"}"#,
            r#"{"data":[{"id":"mon_2"}],"hasMore":false,"nextCursor":null}"#,
        ];
        for (idx, response_body) in responses.iter().enumerate() {
            let started = Instant::now();
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(accepted) => break accepted,
                    Err(err)
                        if err.kind() == ErrorKind::WouldBlock
                            && started.elapsed() < Duration::from_secs(10) =>
                    {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(err) => {
                        panic!("failed to accept local monitors pagination test request: {err}")
                    }
                }
            };
            let request = String::from_utf8_lossy(&read_http_request(&mut stream)).into_owned();
            if idx == 0 {
                assert!(
                    request.starts_with("GET /v0/monitors?websetId=ws_abc&limit=1 "),
                    "unexpected first page request:\n{request}"
                );
            } else {
                assert!(
                    request.starts_with("GET /v0/monitors?websetId=ws_abc&limit=1&cursor=cur2 "),
                    "unexpected second page request:\n{request}"
                );
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            )
            .unwrap();
            stream.flush().unwrap();
        }
    });
    (format!("http://{addr}"), server)
}

#[test]
fn websets_monitors_list_all_preserves_webset_id_filter_across_pages() {
    let (base_url, server) = local_paginated_websets_monitors_list_server();
    let output = run_owned(&[
        "websets".into(),
        "monitors".into(),
        "list".into(),
        "--all".into(),
        "--limit".into(),
        "1".into(),
        "--webset-id".into(),
        "ws_abc".into(),
        "--ndjson".into(),
        "--base-url".into(),
        base_url,
        "--api-key".into(),
        "test-key-abcdef12".into(),
        "--compact".into(),
    ]);
    server
        .join()
        .expect("local websets monitors pagination test server panicked");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn websets_webhooks_create_update_delete_dry_run_and_secret_output() {
    let create = run_ok_json(&[
        "websets",
        "webhooks",
        "create",
        "--url",
        "https://example.com/hook",
        "--event",
        "webset.item.created",
        "--event",
        "monitor.created",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(create["command"], "websets webhooks create");
    assert_eq!(create["data"]["request"]["path"], "/v0/webhooks");
    let body = &create["data"]["request"]["body"];
    assert_eq!(body["url"], "https://example.com/hook");
    assert_eq!(body["events"][0], "webset.item.created");
    assert_eq!(body["events"][1], "monitor.created");
    assert!(create["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["code"] == "webhook_secret_ephemeral"));

    let missing = run(&[
        "websets",
        "webhooks",
        "create",
        "--url",
        "https://example.com/hook",
        "--compact",
    ]);
    assert_eq!(missing.status.code(), Some(1));

    let empty_events_body = run(&[
        "websets",
        "webhooks",
        "create",
        "--body",
        r#"{"url":"https://example.com/hook","events":[]}"#,
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(empty_events_body.status.code(), Some(1));

    let update = run_ok_json(&[
        "websets",
        "webhooks",
        "update",
        "wh_abc",
        "--url",
        "https://example.com/new-hook",
        "--event",
        "webset.created",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(update["data"]["request"]["path"], "/v0/webhooks/wh_abc");
    assert_eq!(
        update["data"]["request"]["body"]["url"],
        "https://example.com/new-hook"
    );

    let empty_update_events = run(&[
        "websets",
        "webhooks",
        "update",
        "wh_abc",
        "--body",
        r#"{"events":[]}"#,
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(empty_update_events.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&empty_update_events.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");

    let delete_live = run(&["websets", "webhooks", "delete", "wh_abc", "--compact"]);
    assert_eq!(delete_live.status.code(), Some(9));

    let attempts = run_ok_json(&[
        "websets",
        "webhooks",
        "attempts",
        "list",
        "wh_abc",
        "--limit",
        "5",
        "--event-type",
        "webset.created",
        "--successful",
        "true",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(
        attempts["data"]["request"]["path"],
        "/v0/webhooks/wh_abc/attempts"
    );
    let query = attempts["data"]["request"]["query"].as_array().unwrap();
    assert!(query
        .iter()
        .any(|entry| { entry["name"] == "eventType" && entry["value"] == "webset.created" }));
    assert!(query
        .iter()
        .any(|entry| { entry["name"] == "successful" && entry["value"] == "true" }));
}

fn local_paginated_websets_webhook_attempts_server() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let responses = [
            r#"{"data":[{"id":"attempt_1"}],"hasMore":true,"nextCursor":"cur2"}"#,
            r#"{"data":[{"id":"attempt_2"}],"hasMore":false,"nextCursor":null}"#,
        ];
        for (idx, response_body) in responses.iter().enumerate() {
            let started = Instant::now();
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(accepted) => break accepted,
                    Err(err)
                        if err.kind() == ErrorKind::WouldBlock
                            && started.elapsed() < Duration::from_secs(10) =>
                    {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(err) => {
                        panic!(
                            "failed to accept local webhook attempts pagination test request: {err}"
                        )
                    }
                }
            };
            let request = String::from_utf8_lossy(&read_http_request(&mut stream)).into_owned();
            if idx == 0 {
                assert!(
                    request.starts_with(
                        "GET /v0/webhooks/wh_abc/attempts?eventType=webset.created&successful=false&limit=1 "
                    ),
                    "unexpected first page request:\n{request}"
                );
            } else {
                assert!(
                    request.starts_with(
                        "GET /v0/webhooks/wh_abc/attempts?eventType=webset.created&successful=false&limit=1&cursor=cur2 "
                    ),
                    "unexpected second page request:\n{request}"
                );
            }
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            )
            .unwrap();
            stream.flush().unwrap();
        }
    });
    (format!("http://{addr}"), server)
}

#[test]
fn websets_webhook_attempts_list_all_preserves_filters_across_pages() {
    let (base_url, server) = local_paginated_websets_webhook_attempts_server();
    let output = run_owned(&[
        "websets".into(),
        "webhooks".into(),
        "attempts".into(),
        "list".into(),
        "wh_abc".into(),
        "--all".into(),
        "--limit".into(),
        "1".into(),
        "--event-type".into(),
        "webset.created".into(),
        "--successful".into(),
        "false".into(),
        "--ndjson".into(),
        "--base-url".into(),
        base_url,
        "--api-key".into(),
        "test-key-abcdef12".into(),
        "--compact".into(),
    ]);
    server
        .join()
        .expect("local websets webhook attempts pagination test server panicked");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn websets_webhooks_create_live_captures_secret_and_redacts_stdout() {
    let response = br#"{"id":"wh_test","secret":"whsec_websets_capture_12345"}"#;
    let (base_url, server) = local_json_server(
        |request| {
            assert!(request.starts_with("POST /v0/webhooks "));
        },
        response,
    );
    let secret_path = temp_path("websets-webhook-secret").join("secret.txt");
    let output = run_owned(&[
        "websets".into(),
        "webhooks".into(),
        "create".into(),
        "--url".into(),
        "https://example.com/hook".into(),
        "--event".into(),
        "webset.item.created".into(),
        "--secret-output".into(),
        secret_path.to_string_lossy().into_owned(),
        "--base-url".into(),
        base_url,
        "--api-key".into(),
        "test-key-abcdef12".into(),
        "--compact".into(),
    ]);
    server
        .join()
        .expect("local websets webhook create server panicked");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    assert!(!stdout.contains("whsec_websets_capture_12345"));
    assert!(stdout.contains("<redacted>"));
    let secret = fs::read_to_string(&secret_path).expect("secret file");
    assert_eq!(secret, "whsec_websets_capture_12345");
}

#[test]
fn websets_webhooks_create_secret_output_requires_secret_field() {
    let (base_url, server) = local_json_server(
        |request| {
            assert!(request.starts_with("POST /v0/webhooks "));
        },
        br#"{"id":"wh_test"}"#,
    );
    let secret_path = temp_path("websets-webhook-secret-missing").join("secret.txt");
    let output = run_owned(&[
        "websets".into(),
        "webhooks".into(),
        "create".into(),
        "--url".into(),
        "https://example.com/hook".into(),
        "--event".into(),
        "webset.item.created".into(),
        "--secret-output".into(),
        secret_path.to_string_lossy().into_owned(),
        "--base-url".into(),
        base_url,
        "--api-key".into(),
        "test-key-abcdef12".into(),
        "--compact".into(),
    ]);
    server
        .join()
        .expect("local websets webhook create server panicked");
    assert!(!output.status.success());
    assert!(!secret_path.exists());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("secret"));
}

#[test]
fn websets_webhooks_create_raw_is_rejected_for_secret_safety() {
    let output = run(&[
        "websets",
        "webhooks",
        "create",
        "--url",
        "https://example.com/hook",
        "--event",
        "webset.item.created",
        "--raw",
        "--compact",
    ]);
    assert_eq!(output.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_flag_combination");
}

#[test]
fn websets_webhooks_create_ambiguous_failure_records_pending_run() {
    let dir = temp_path("websets-webhook-create-pending");
    let pending_path = dir.join("pending-runs.jsonl");
    let pending_path_string = pending_path.to_string_lossy().into_owned();
    let base_url = closed_local_base_url();
    let output = run_with_env(
        &[
            "websets",
            "webhooks",
            "create",
            "--url",
            "https://example.com/hook",
            "--event",
            "webset.item.created",
            "--base-url",
            base_url.as_str(),
            "--api-key",
            "test-key-abcdef12",
            "--compact",
        ],
        &[("EXA_AGENT_PENDING_RUNS", pending_path_string.as_str())],
    );
    assert!(!output.status.success());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["details"]["pendingRunWritten"], true);
    assert_pending_record(&pending_path, "websets webhooks create", "/v0/webhooks");
}

#[test]
fn parse_admin_keys_commands() {
    assert_path(
        &[
            "admin",
            "keys",
            "create",
            "--name",
            "ci-key",
            "--rate-limit",
            "100",
        ],
        "admin keys create",
    );
    assert_path(&["admin", "keys", "list"], "admin keys list");
    assert_path(&["admin", "keys", "get", "key_abc"], "admin keys get");
    assert_path(
        &[
            "admin",
            "keys",
            "update",
            "key_abc",
            "--name",
            "renamed",
            "--clear-budget-cents",
        ],
        "admin keys update",
    );
    parses(&["admin", "keys", "delete", "key_abc", "--confirm", "key_abc"]);
    parses(&[
        "admin",
        "keys",
        "usage",
        "key_abc",
        "--start-date",
        "2026-01-01",
        "--end-date",
        "2026-06-01",
        "--group-by",
        "DAY",
    ]);
    assert_eq!(
        parse_err(&["admin", "keys", "usage", "key_abc", "--group-by", "decade"]).kind(),
        clap::error::ErrorKind::InvalidValue
    );
}

#[test]
fn admin_keys_dry_run_builds_requests() {
    let create = run_ok_json(&[
        "admin",
        "keys",
        "create",
        "--name",
        "ci-key",
        "--rate-limit",
        "100",
        "--budget-cents",
        "500",
        "--dry-run",
        "--print-request",
        "--compact",
    ]);
    assert_eq!(create["command"], "admin keys create");
    assert_eq!(create["operation"]["method"], "POST");
    assert_eq!(create["operation"]["path"], "/api-keys");
    assert_eq!(create["operation"]["operationId"], "create-api-key");
    assert_eq!(create["operation"]["source"], "team-management.json");
    assert_eq!(create["data"]["request"]["body"]["name"], "ci-key");
    assert_eq!(create["data"]["request"]["body"]["rateLimit"], 100);
    assert_eq!(create["data"]["request"]["body"]["budgetCents"], 500);

    let keyed_create = run_ok_json(&[
        "admin",
        "keys",
        "create",
        "--name",
        "ci-key",
        "--idempotency-key",
        "idem-admin-create",
        "--dry-run",
        "--print-request",
        "--compact",
    ]);
    let headers = keyed_create["data"]["request"]["headers"]
        .as_array()
        .expect("headers");
    assert!(headers.iter().any(|header| {
        header["name"] == "Idempotency-Key" && header["value"] == "idem-admin-create"
    }));

    let list = run_ok_json(&[
        "admin",
        "keys",
        "list",
        "--dry-run",
        "--print-request",
        "--compact",
    ]);
    assert_eq!(list["command"], "admin keys list");
    assert_eq!(list["data"]["request"]["method"], "GET");
    assert_eq!(list["data"]["request"]["path"], "/api-keys");
    assert!(list["data"]["request"]["body"].is_null());

    let get = run_ok_json(&[
        "admin",
        "keys",
        "get",
        "key/with/slash",
        "--dry-run",
        "--print-request",
        "--compact",
    ]);
    assert_eq!(
        get["data"]["request"]["path"],
        "/api-keys/key%2Fwith%2Fslash"
    );

    let update = run_ok_json(&[
        "admin",
        "keys",
        "update",
        "key_abc",
        "--name",
        "renamed",
        "--clear-budget-cents",
        "--dry-run",
        "--print-request",
        "--compact",
    ]);
    assert_eq!(update["command"], "admin keys update");
    assert_eq!(update["operation"]["method"], "PUT");
    assert_eq!(update["data"]["request"]["path"], "/api-keys/key_abc");
    assert_eq!(update["data"]["request"]["body"]["name"], "renamed");
    assert!(update["data"]["request"]["body"]["budgetCents"].is_null());

    let delete = run_ok_json(&[
        "admin",
        "keys",
        "delete",
        "key_abc",
        "--dry-run",
        "--print-request",
        "--compact",
    ]);
    assert_eq!(delete["command"], "admin keys delete");
    assert_eq!(delete["operation"]["method"], "DELETE");
    assert_eq!(delete["data"]["request"]["path"], "/api-keys/key_abc");
    assert!(delete["data"]["request"]["body"].is_null());

    let usage_output = run_with_env(
        &[
            "admin",
            "keys",
            "usage",
            "key_abc",
            "--start-date",
            "2026-01-01",
            "--end-date",
            "2026-01-31",
            "--group-by",
            "DAY",
            "--dry-run",
            "--print-request",
            "--compact",
        ],
        &[("SOURCE_DATE_EPOCH", "1782777600")],
    );
    assert!(
        usage_output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&usage_output.stderr)
    );
    let usage: serde_json::Value = serde_json::from_slice(&usage_output.stdout).unwrap();
    assert_eq!(usage["command"], "admin keys usage");
    assert_eq!(usage["data"]["request"]["path"], "/api-keys/key_abc/usage");
    assert_eq!(
        usage["data"]["request"]["query"],
        serde_json::json!([
            {"name":"start_date","value":"2026-01-01"},
            {"name":"end_date","value":"2026-01-31"},
            {"name":"group_by","value":"day"}
        ])
    );
}

#[test]
fn admin_keys_delete_requires_confirm_by_id() {
    for args in [
        vec!["admin", "keys", "delete", "key_abc", "--compact"],
        vec![
            "admin",
            "keys",
            "delete",
            "key_abc",
            "--confirm",
            "other_key",
            "--compact",
        ],
    ] {
        let output = run(&args);
        assert_eq!(output.status.code(), Some(9));
        assert!(output.stdout.is_empty());
        let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(stderr["error"]["code"], "confirmation_required");
        assert!(stderr["error"]["suggestedCommand"]
            .as_str()
            .unwrap()
            .contains("--confirm key_abc"));
    }
}

#[test]
fn admin_keys_update_and_body_validation() {
    let empty = run(&["admin", "keys", "update", "key_abc", "--compact"]);
    assert_eq!(empty.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&empty.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "missing_required_argument");

    let negative_budget = run(&[
        "admin",
        "keys",
        "create",
        "--set",
        "budgetCents=-1",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(negative_budget.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&negative_budget.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");

    let bad_rate = run(&[
        "admin",
        "keys",
        "update",
        "key_abc",
        "--set",
        "rateLimit=-1",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(bad_rate.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&bad_rate.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");
}

#[test]
fn admin_keys_usage_validates_dates_and_lookback() {
    let env = [("SOURCE_DATE_EPOCH", "1782777600")]; // 2026-06-30T00:00:00Z
    let ok = run_with_env(
        &[
            "admin",
            "keys",
            "usage",
            "key_abc",
            "--start-date",
            "2026-01-01",
            "--end-date",
            "2026-06-30",
            "--dry-run",
            "--compact",
        ],
        &env,
    );
    assert!(
        ok.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&ok.stderr)
    );

    let boundary_midday = run_with_env(
        &[
            "admin",
            "keys",
            "usage",
            "key_abc",
            "--start-date",
            "2026-01-01",
            "--end-date",
            "2026-06-30",
            "--dry-run",
            "--compact",
        ],
        &[("SOURCE_DATE_EPOCH", "1782820800")], // 2026-06-30T12:00:00Z
    );
    assert!(
        boundary_midday.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&boundary_midday.stderr)
    );

    let reversed = run_with_env(
        &[
            "admin",
            "keys",
            "usage",
            "key_abc",
            "--start-date",
            "2026-06-30",
            "--end-date",
            "2026-01-01",
            "--dry-run",
            "--compact",
        ],
        &env,
    );
    assert_eq!(reversed.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&reversed.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");

    let too_old = run_with_env(
        &[
            "admin",
            "keys",
            "usage",
            "key_abc",
            "--start-date",
            "2025-12-31",
            "--dry-run",
            "--compact",
        ],
        &env,
    );
    assert_eq!(too_old.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&too_old.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");

    let end_too_old = run_with_env(
        &[
            "admin",
            "keys",
            "usage",
            "key_abc",
            "--end-date",
            "2025-12-31",
            "--dry-run",
            "--compact",
        ],
        &env,
    );
    assert_eq!(end_too_old.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&end_too_old.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");

    let future_end = run_with_env(
        &[
            "admin",
            "keys",
            "usage",
            "key_abc",
            "--start-date",
            "2026-06-01",
            "--end-date",
            "2100-01-01",
            "--dry-run",
            "--compact",
        ],
        &env,
    );
    assert_eq!(future_end.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&future_end.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");

    let range_too_wide = run_with_env(
        &[
            "admin",
            "keys",
            "usage",
            "key_abc",
            "--start-date",
            "2025-12-31",
            "--end-date",
            "2026-06-30",
            "--dry-run",
            "--compact",
        ],
        &env,
    );
    assert_eq!(range_too_wide.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&range_too_wide.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");
}

#[test]
fn admin_keys_use_service_key_and_admin_base_url() {
    let (base_url, server) = local_json_server(
        |request| {
            assert!(
                request.starts_with("GET /api-keys "),
                "unexpected admin request:\n{request}"
            );
            let lower = request.to_ascii_lowercase();
            assert!(
                lower.contains("x-api-key: svc-admin-secret"),
                "expected service key header:\n{request}"
            );
            assert!(
                !lower.contains("test-key-abcdef12"),
                "admin request used normal API key:\n{request}"
            );
        },
        br#"{"apiKeys":[]}"#,
    );
    let output = run_with_env(
        &["admin", "keys", "list", "--compact"],
        &[
            ("EXA_SERVICE_KEY", "svc-admin-secret"),
            ("EXA_API_KEY", "test-key-abcdef12"),
            ("EXA_ADMIN_BASE_URL", base_url.as_str()),
        ],
    );
    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    server.join().unwrap();

    let (base_url, server) = local_json_server(
        |request| {
            assert!(
                request.starts_with("GET /api-keys "),
                "unexpected admin request:\n{request}"
            );
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("x-api-key: svc-admin-secret"),
                "expected service key header:\n{request}"
            );
        },
        br#"{"apiKeys":[]}"#,
    );
    let output = run_with_env(
        &[
            "admin",
            "keys",
            "list",
            "--base-url",
            "http://127.0.0.1:9",
            "--compact",
        ],
        &[
            ("EXA_SERVICE_KEY", "svc-admin-secret"),
            ("EXA_ADMIN_BASE_URL", base_url.as_str()),
        ],
    );
    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    server.join().unwrap();

    let dir = temp_path("admin-profile-base-url");
    let config = dir.join("config.toml");
    let (base_url, server) = local_json_server(
        |request| {
            assert!(
                request.starts_with("GET /api-keys "),
                "unexpected admin request:\n{request}"
            );
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("x-api-key: svc-admin-secret"),
                "expected service key header:\n{request}"
            );
        },
        br#"{"apiKeys":[]}"#,
    );
    fs::write(
        &config,
        format!("[profiles.admin]\nadmin_base_url = \"{base_url}\"\n"),
    )
    .unwrap();
    let output = run_with_env(
        &["--profile", "admin", "admin", "keys", "list", "--compact"],
        &[
            ("EXA_SERVICE_KEY", "svc-admin-secret"),
            ("EXA_AGENT_CONFIG", config.to_str().unwrap()),
        ],
    );
    assert!(
        output.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    server.join().unwrap();

    let missing_service = run_with_env(
        &["admin", "keys", "list", "--compact"],
        &[("EXA_API_KEY", "test-key-abcdef12")],
    );
    assert_eq!(missing_service.status.code(), Some(2));
    let stderr: serde_json::Value = serde_json::from_slice(&missing_service.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "not_authenticated");
    assert!(stderr["error"]["details"]["checked"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value == "EXA_SERVICE_KEY"));

    let api_shaped_service = run_with_env(
        &["admin", "keys", "list", "--compact"],
        &[
            ("EXA_SERVICE_KEY", "00000000-0000-0000-0000-000000000000"),
            ("EXA_ADMIN_BASE_URL", closed_local_base_url().as_str()),
        ],
    );
    assert_eq!(api_shaped_service.status.code(), Some(2));
    let stderr: serde_json::Value = serde_json::from_slice(&api_shaped_service.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "key_scope_mismatch");
}

#[test]
fn service_shaped_api_key_is_rejected_for_api_commands() {
    let output = run_with_env(
        &["search", "hello", "--compact"],
        &[("EXA_API_KEY", "svc-admin-secret")],
    );
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "key_scope_mismatch");
    assert!(stderr["error"]["message"]
        .as_str()
        .unwrap()
        .contains("service/admin key"));

    let status = run_with_env(
        &["auth", "status", "--compact"],
        &[("EXA_API_KEY", "svc-admin-secret")],
    );
    assert!(status.status.success());
    let stdout: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert!(stdout["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning.as_str().unwrap().contains("service key")));
}

#[test]
fn admin_keys_create_safety_edges() {
    let raw = run(&["admin", "keys", "create", "--raw", "--compact"]);
    assert_eq!(raw.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&raw.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_flag_combination");

    let base_url = closed_local_base_url();

    // Without --secret-output the minted key would be unretrievable (it is never
    // printed to stdout), so the command is refused before any network call.
    let no_capture = run_with_env(
        &["admin", "keys", "create", "--name", "ci-key", "--compact"],
        &[
            ("EXA_SERVICE_KEY", "svc-admin-secret"),
            ("EXA_ADMIN_BASE_URL", base_url.as_str()),
        ],
    );
    assert!(!no_capture.status.success());
    let stderr: serde_json::Value = serde_json::from_slice(&no_capture.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "missing_required_argument");

    // With --secret-output the file is reserved before the request; an ambiguous
    // failure against the closed endpoint records a pending-run for recovery.
    let dir = temp_path("admin-key-create-pending");
    let pending_path = dir.join("pending-runs.jsonl");
    let pending_path_string = pending_path.to_string_lossy().into_owned();
    let secret_path = dir.join("key.secret");
    let secret_path_string = secret_path.to_string_lossy().into_owned();
    let output = run_with_env(
        &[
            "admin",
            "keys",
            "create",
            "--name",
            "ci-key",
            "--secret-output",
            secret_path_string.as_str(),
            "--compact",
        ],
        &[
            ("EXA_SERVICE_KEY", "svc-admin-secret"),
            ("EXA_ADMIN_BASE_URL", base_url.as_str()),
            ("EXA_AGENT_PENDING_RUNS", pending_path_string.as_str()),
        ],
    );
    assert!(!output.status.success());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["details"]["pendingRunWritten"], true);
    assert_pending_record(&pending_path, "admin keys create", "/api-keys");
    // The reservation is rolled back when the request never succeeds.
    assert!(!secret_path.exists());
}

#[test]
fn admin_keys_create_captures_key_to_file_and_redacts_stdout() {
    let response = br#"{"id":"key_test","apiKey":"exa_minted_live_secret_123"}"#;
    let (base_url, server) = local_json_server(
        |request| {
            assert!(request.starts_with("POST /api-keys "));
        },
        response,
    );
    let secret_path = temp_path("admin-key-secret").join("key.secret");
    let secret_path_string = secret_path.to_string_lossy().into_owned();
    let output = run_with_env(
        &[
            "admin",
            "keys",
            "create",
            "--name",
            "ci",
            "--secret-output",
            secret_path_string.as_str(),
            "--compact",
        ],
        &[
            ("EXA_SERVICE_KEY", "svc-admin-secret"),
            ("EXA_ADMIN_BASE_URL", base_url.as_str()),
        ],
    );
    server.join().expect("local admin create server panicked");
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
    assert!(
        !stdout.contains("exa_minted_live_secret_123"),
        "minted key must never appear on stdout: {stdout}"
    );
    let secret = fs::read_to_string(&secret_path).expect("secret file");
    assert_eq!(secret, "exa_minted_live_secret_123");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&secret_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}

#[test]
fn parse_macros_ask_and_fetch() {
    assert_path(&["ask", "What changed in AI this week?"], "ask");
    assert_path(&["fetch", "https://exa.ai", "https://docs.exa.ai"], "fetch");
    let ask = parses(&["ask", "What changed in AI this week?"]);
    let Command::Ask(args) = ask.command else {
        panic!("expected ask command");
    };
    assert_eq!(args.question, "What changed in AI this week?");

    let fetch = parses(&["fetch", "https://exa.ai", "https://docs.exa.ai"]);
    let Command::Fetch(args) = fetch.command else {
        panic!("expected fetch command");
    };
    assert_eq!(args.urls, ["https://exa.ai", "https://docs.exa.ai"]);
    assert_eq!(
        parse_err(&["fetch"]).kind(),
        clap::error::ErrorKind::MissingRequiredArgument
    );
}

#[test]
fn ask_dry_run_expands_to_typed_answer_request() {
    let json = run_ok_json(&[
        "ask",
        "What is Exa?",
        "--dry-run",
        "--print-request",
        "--compact",
    ]);
    let body = &json["data"]["request"]["body"];
    assert_eq!(json["command"], "answer");
    assert_eq!(json["data"]["request"]["path"], "/answer");
    assert_eq!(body["query"], "What is Exa?");
    assert_eq!(body["text"], true);
    assert_eq!(json["data"]["expandsTo"], "answer 'What is Exa?' --text");
    assert_eq!(json["data"]["expands_to"], "answer 'What is Exa?' --text");

    let quoted = run_ok_json(&[
        "ask",
        "What's Exa?",
        "--dry-run",
        "--print-request",
        "--compact",
    ]);
    assert_eq!(
        quoted["data"]["expandsTo"],
        "answer 'What'\\''s Exa?' --text"
    );
}

#[test]
fn ask_live_response_does_not_include_macro_metadata() {
    let (base_url, server) = local_json_server(
        |request_text| {
            assert!(
                request_text.starts_with("POST /answer "),
                "unexpected request:\n{request_text}"
            );
            assert!(
                request_text
                    .to_ascii_lowercase()
                    .contains("x-api-key: test-key-abcdef12"),
                "request did not include explicit test API key:\n{request_text}"
            );
            assert!(
                request_text.contains(r#""query":"What is Exa?""#)
                    && request_text.contains(r#""text":true"#),
                "ask macro did not send the typed answer body:\n{request_text}"
            );
        },
        br#"{"answer":"done","citations":[]}"#,
    );
    let output = run(&[
        "ask",
        "What is Exa?",
        "--api-key",
        "test-key-abcdef12",
        "--base-url",
        base_url.as_str(),
        "--compact",
    ]);
    server.join().expect("local test server panicked");

    assert!(
        output.status.success(),
        "expected live local ask success\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let upstream = serde_json::json!({"answer":"done","citations":[]});
    assert_eq!(json["command"], "answer");
    assert_eq!(json["data"], upstream);
    assert!(json["data"].get("expandsTo").is_none());
    assert!(json["data"].get("expands_to").is_none());
    assert_eq!(json["dataHash"], transport::data_hash(&upstream).unwrap());
}

#[test]
fn fetch_live_response_does_not_include_macro_metadata() {
    let upstream =
        br#"{"results":[{"url":"https://exa.ai","text":"ok"}],"statuses":[{"id":"https://exa.ai","status":"success"}]}"#;
    let (base_url, server) = local_json_server(
        |request_text| {
            assert!(
                request_text.starts_with("POST /contents "),
                "unexpected request:\n{request_text}"
            );
            assert!(
                request_text
                    .to_ascii_lowercase()
                    .contains("x-api-key: test-key-abcdef12"),
                "request did not include explicit test API key:\n{request_text}"
            );
            assert!(
                request_text.contains(r#""urls":["https://exa.ai"]"#)
                    && request_text.contains(r#""text":true"#)
                    && request_text.contains(r#""summary":{"query":"Summarize the page"}"#),
                "fetch macro did not send the typed contents body:\n{request_text}"
            );
        },
        upstream,
    );
    let output = run(&[
        "fetch",
        "https://exa.ai",
        "--api-key",
        "test-key-abcdef12",
        "--base-url",
        base_url.as_str(),
        "--compact",
    ]);
    server.join().expect("local test server panicked");

    assert!(
        output.status.success(),
        "expected live local fetch success\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let upstream: serde_json::Value = serde_json::from_slice(upstream).unwrap();
    assert_eq!(json["command"], "contents");
    assert_eq!(json["data"], upstream);
    assert!(json["data"].get("expandsTo").is_none());
    assert!(json["data"].get("expands_to").is_none());
    assert_eq!(json["dataHash"], transport::data_hash(&upstream).unwrap());
}

#[test]
fn fetch_dry_run_expands_to_typed_contents_request() {
    let json = run_ok_json(&[
        "fetch",
        "https://exa.ai",
        "https://docs.exa.ai/search?q=rust&sort=new",
        "--dry-run",
        "--compact",
    ]);
    let body = &json["data"]["request"]["body"];
    assert_eq!(json["command"], "contents");
    assert_eq!(json["data"]["request"]["path"], "/contents");
    assert_eq!(
        body["urls"],
        serde_json::json!([
            "https://exa.ai",
            "https://docs.exa.ai/search?q=rust&sort=new"
        ])
    );
    assert_eq!(body["text"], true);
    assert_eq!(body["summary"]["query"], "Summarize the page");
    assert_eq!(
        json["data"]["expandsTo"],
        "contents 'https://exa.ai' 'https://docs.exa.ai/search?q=rust&sort=new' --text --summary-query 'Summarize the page'"
    );
    assert_eq!(
        json["data"]["expands_to"],
        "contents 'https://exa.ai' 'https://docs.exa.ai/search?q=rust&sort=new' --text --summary-query 'Summarize the page'"
    );
}

#[test]
fn parse_preserves_global_flags_with_leaf_command() {
    let cli = parses(&[
        "--json",
        "--profile",
        "work",
        "search",
        "test query",
        "--correlation-id",
        "run-1",
        "--idempotency-key",
        "idem-1",
        "--input",
        "input.jsonl",
        "--input-format",
        "JSONL",
        "--set",
        "foo.bar=1",
        "--max-output-bytes",
        "1024",
    ]);
    assert!(cli.globals.json);
    assert!(cli.globals.retry_after);
    assert_eq!(cli.globals.profile.as_deref(), Some("work"));
    assert_eq!(cli.globals.correlation_id.as_deref(), Some("run-1"));
    assert_eq!(cli.globals.idempotency_key.as_deref(), Some("idem-1"));
    assert!(matches!(cli.command, Command::Search(_)));
}

#[test]
fn debug_redacts_global_secret_values() {
    let cli = parses(&[
        "--api-key",
        "exa-secret-key",
        "--service-key",
        "service-secret-key",
        "--header",
        "Authorization: Bearer header-secret",
        "--header",
        "x-exa-service-key: service-key-secret",
        "--set",
        "webhookSecret=set-secret",
        "--body",
        "{\"token\":\"body-secret\"}",
        "raw",
        "GET",
        "/search",
        "--query",
        "token=query-secret",
    ]);
    let dbg = format!("{cli:?}");
    assert!(!dbg.contains("exa-secret-key"));
    assert!(!dbg.contains("service-secret-key"));
    assert!(!dbg.contains("header-secret"));
    assert!(!dbg.contains("service-key-secret"));
    assert!(!dbg.contains("set-secret"));
    assert!(!dbg.contains("body-secret"));
    assert!(!dbg.contains("query-secret"));
    assert!(dbg.contains("<redacted>"));
}
