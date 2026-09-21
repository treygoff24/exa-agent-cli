//! Loopback-only follow-up tests; these do not establish upstream API behavior.

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn temporary() -> std::path::PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "exa-next-actions-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&directory).unwrap();
    directory
}

fn run(args: &[&str], offline: bool) -> Value {
    serde_json::from_slice(&run_output(args, offline)).unwrap()
}

fn run_output(args: &[&str], offline: bool) -> Vec<u8> {
    let directory = temporary();
    std::fs::write(directory.join("config.toml"), "[profiles.audit]\n").unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_exa-agent"));
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("EXA_") {
            command.env_remove(name);
        }
    }
    for (key, file) in [
        ("CONFIG", "config.toml"),
        ("CREDENTIALS", "credentials.json"),
        ("PRESETS", "presets.toml"),
        ("LOCAL_PRESETS", "local.toml"),
        ("STATE", "state"),
    ] {
        command.env(format!("EXA_AGENT_{key}"), directory.join(file));
    }
    if offline {
        command.env("EXA_AGENT_NO_NETWORK", "1");
    }
    let output = command.args(args).output().unwrap();
    std::fs::remove_dir_all(directory).unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}

fn server(response: Value) -> (String, std::thread::JoinHandle<String>) {
    server_pages(vec![response])
}

fn server_pages(responses: Vec<Value>) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let handle = std::thread::spawn(move || {
        let mut requests = String::new();
        for response in responses {
            let deadline = Instant::now() + Duration::from_secs(10);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(10))
                    }
                    Err(error) => panic!("accept loopback request: {error}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = Vec::new();
            loop {
                let mut buffer = [0; 4096];
                let count = socket.read(&mut buffer).unwrap();
                assert_ne!(count, 0, "incomplete HTTP request");
                request.extend_from_slice(&buffer[..count]);
                if let Some(end) = request.windows(4).position(|bytes| bytes == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..end]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                        .unwrap_or(0);
                    if request.len() >= end + 4 + length {
                        break;
                    }
                }
            }
            let body = response.to_string();
            write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            requests.push_str(&String::from_utf8(request).unwrap());
        }
        requests
    });
    (url, handle)
}

// Parse only the quoted-word grammar emitted by this CLI. Refuse shell operators rather
// than executing returned commands, so an escaping regression cannot run fixture text.
fn words(command: &str) -> Vec<String> {
    let (mut quoted, mut escaped, mut started) = (false, false, false);
    let (mut word, mut words) = (String::new(), Vec::new());
    for ch in command.chars() {
        if escaped {
            word.push(ch);
            escaped = false;
        } else if ch == '\'' {
            quoted = !quoted;
            started = true;
        } else if !quoted && ch == '\\' {
            escaped = true;
            started = true;
        } else if !quoted && ch.is_whitespace() {
            assert!(!matches!(ch, '\n' | '\r'), "shell command separator");
            if started {
                words.push(std::mem::take(&mut word));
                started = false;
            }
        } else {
            assert!(
                quoted || !";|&$`()<>\"".contains(ch),
                "unquoted shell operator in {command}"
            );
            word.push(ch);
            started = true;
        }
    }
    assert!(!quoted && !escaped, "unterminated shell quoting");
    if started {
        words.push(word);
    }
    words
}

fn preview(action: &str) -> Value {
    let words = words(action);
    assert_eq!(words[0], "exa-agent");
    exa_agent_cli::cli::Cli::try_parse_from(words.clone()).expect("typed follow-up parses");
    let mut args = vec!["--json", "--dry-run", "--print-request"];
    args.extend(words.iter().skip(1).map(String::as_str));
    run(&args, true)
}

fn assert_all_exa_actions_parse(envelope: &Value) -> usize {
    let commands = envelope["nextActions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|action| action["command"].as_str())
        .chain(
            envelope["warnings"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|warning| warning["suggestedCommand"].as_str()),
        );
    let mut parsed = 0;
    for command in commands {
        let argv = words(command);
        if argv.first().map(String::as_str) != Some("exa-agent") {
            continue;
        }
        exa_agent_cli::cli::Cli::try_parse_from(argv)
            .unwrap_or_else(|error| panic!("unparseable emitted command `{command}`: {error}"));
        parsed += 1;
    }
    parsed
}

#[test]
fn warning_derived_recovery_actions_parse_through_the_real_cli() {
    let (url, handle) = server(json!({
        "results": [{"url":"https://a.test","text":"ok"}],
        "statuses": [
            {"id":"https://a.test","status":"success"},
            {"id":"https://b.test","status":"error","error":{"tag":"CRAWL_TIMEOUT"}}
        ]
    }));
    let response = run(
        &[
            "contents",
            "https://a.test",
            "https://b.test",
            "--base-url",
            &url,
            "--api-key",
            "fixture-secret",
            "--json",
        ],
        false,
    );
    handle.join().unwrap();
    let suggested = response["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|warning| warning["suggestedCommand"].is_string())
        .count();
    assert!(
        suggested > 0,
        "fixture emitted no warning recovery: {response}"
    );
    assert_eq!(assert_all_exa_actions_parse(&response), suggested * 2);
}

#[test]
fn async_create_followups_use_returned_ids_and_quote_shell_metacharacters() {
    for (command, api_path, id, expected_paths) in [
        (
            "agent",
            "/agent/runs",
            "--run/'$(echo SAFE);id",
            vec![
                "/agent/runs/--run%2F%27%24%28echo%20SAFE%29%3Bid",
                "/agent/runs/--run%2F%27%24%28echo%20SAFE%29%3Bid/events",
            ],
        ),
        (
            "websets",
            "/websets/v0/websets",
            "--webset/part",
            vec![
                "/websets/v0/websets/--webset%2Fpart",
                "/websets/v0/websets/--webset%2Fpart/items",
            ],
        ),
    ] {
        let (url, handle) = server(json!({"id":id,"status":"running"}));
        let mut args = if command == "agent" {
            vec!["agent", "runs", "create", "AI"]
        } else {
            vec!["websets", "create", "--query", "AI", "--count", "1"]
        };
        args.extend(["--base-url", &url, "--api-key", "fixture-secret", "--json"]);
        let response = run(&args, false);
        assert!(handle
            .join()
            .unwrap()
            .starts_with(&format!("POST {api_path} ")));
        let actions = response["nextActions"].as_array().unwrap();
        assert_eq!(actions.len(), expected_paths.len());
        for (action, expected) in actions.iter().zip(expected_paths) {
            let command = action["command"].as_str().unwrap();
            let parsed = words(command);
            assert_eq!(parsed.last().unwrap(), id);
            assert_eq!(parsed[parsed.len() - 2], "--");
            assert!(!command.contains("fixture-secret"));
            assert_eq!(preview(command)["data"]["request"]["path"], expected);
        }
    }
}

#[test]
fn pagination_followups_preserve_scope_without_credentials_or_output_overwrite() {
    for capped in [false, true] {
        let (url, handle) =
            server(json!({"data":[{"id":"item1"}],"hasMore":true,"nextCursor":"cursor / next"}));
        let directory = temporary();
        let output_path = directory.join("page.json");
        let mut args = vec![
            "websets",
            "items",
            "list",
            "ws/part",
            "--source-id",
            "src/one",
            "--limit",
            "1",
            "--cursor",
            "old",
            "--profile",
            "audit",
            "--beta",
            "fixture-beta",
            "--base-url",
            &url,
            "--api-key",
            "fixture-secret",
            "--output",
            output_path.to_str().unwrap(),
            "--json",
        ];
        if capped {
            args.extend(["--all", "--max-pages", "1"]);
        }
        run(&args, false);
        let request = handle.join().unwrap();
        assert!(request.starts_with("GET /websets/v0/websets/ws%2Fpart/items?"));
        let saved = std::fs::read(&output_path).unwrap();
        let response: Value = serde_json::from_slice(&saved).unwrap();
        let actions = response["nextActions"].as_array().unwrap();
        assert!(!actions.is_empty(), "missing continuation: {response}");
        let command = actions[0]["command"].as_str().unwrap();
        let parsed = exa_agent_cli::cli::Cli::try_parse_from(words(command)).unwrap();
        assert_eq!(parsed.globals.profile.as_deref(), Some("audit"));
        assert_eq!(parsed.globals.beta.as_deref(), Some("fixture-beta"));
        assert_eq!(parsed.globals.base_url.as_deref(), Some(url.as_str()));
        assert!(parsed.globals.api_key.is_none());
        assert!(parsed.globals.service_key.is_none());
        assert!(parsed.globals.output.is_none());
        assert!(!command.contains("fixture-secret"));
        let result = preview(command);
        assert_eq!(
            result["data"]["request"]["path"],
            "/websets/v0/websets/ws%2Fpart/items"
        );
        let query = result["data"]["request"]["query"].as_array().unwrap();
        for (name, value) in [
            ("sourceId", "src/one"),
            ("limit", "1"),
            ("cursor", "cursor / next"),
        ] {
            assert!(
                query.contains(&json!({"name":name,"value":value})),
                "{query:?}"
            );
        }
        assert_eq!(std::fs::read(&output_path).unwrap(), saved);
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[test]
fn continuations_do_not_drop_private_routing_or_echo_secret_filters() {
    for private_filter in [false, true] {
        let (url, handle) = server(json!({"data":[],"hasMore":true,"nextCursor":"next-page"}));
        let base = if private_filter {
            url
        } else {
            format!("{url}/private-proxy")
        };
        let mut args = vec![
            "monitor",
            "list",
            "--base-url",
            &base,
            "--api-key",
            "fixture-secret",
            "--json",
        ];
        if private_filter {
            args.extend(["--metadata", "api_key=private-filter-value"]);
        }
        let response = run(&args, false);
        handle.join().unwrap();
        assert!(
            response["nextActions"].as_array().unwrap().is_empty(),
            "unsafe follow-up: {response}"
        );
        assert!(response["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning["code"] == "followup_context_required"));
        assert!(!response.to_string().contains("private-filter-value"));
    }
}

#[test]
fn caller_headers_are_not_copied_into_success_followups() {
    let (url, handle) = server(json!({"id":"run_created","status":"running"}));
    let response = run(
        &[
            "agent",
            "runs",
            "create",
            "q",
            "--base-url",
            &url,
            "--api-key",
            "fixture-secret",
            "--header",
            "X-Context: private-context-value",
            "--json",
        ],
        false,
    );
    let request = handle.join().unwrap();
    assert!(
        request.contains("private-context-value"),
        "caller header must still reach its chosen endpoint"
    );
    assert!(response["nextActions"].as_array().unwrap().is_empty());
    assert!(response["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["code"] == "followup_context_required"));
    assert!(!response.to_string().contains("private-context-value"));
}

fn assert_repeated_cursor_stops_without_a_continuation(ndjson: bool) {
    let (url, handle) = server_pages(vec![
        json!({"data":[{"id":"item1"}],"hasMore":true,"nextCursor":"samecursor"}),
        json!({"data":[{"id":"item2"}],"hasMore":true,"nextCursor":"samecursor"}),
    ]);
    let output = run_output(
        &[
            "websets",
            "items",
            "list",
            "ws1",
            "--all",
            "--limit",
            "1",
            "--cursor",
            "originalcursor",
            "--base-url",
            &url,
            "--api-key",
            "fixture-secret",
            if ndjson { "--ndjson" } else { "--json" },
        ],
        false,
    );
    let requests = handle.join().unwrap();
    assert_eq!(
        requests
            .matches("GET /websets/v0/websets/ws1/items?")
            .count(),
        2
    );
    assert!(requests.contains("cursor=originalcursor"));
    assert!(requests.contains("cursor=samecursor"));
    let envelopes: Vec<Value> = if ndjson {
        String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    } else {
        vec![serde_json::from_slice(&output).unwrap()]
    };
    assert_eq!(envelopes.len(), if ndjson { 2 } else { 1 });
    let terminal = envelopes.last().unwrap();
    assert_eq!(terminal["pagination"]["hasMore"], false);
    assert_eq!(terminal["pagination"]["pageCount"], 2);
    assert!(terminal["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["code"] == "pagination_repeated_cursor"));
    assert!(
        terminal["nextActions"].as_array().unwrap().is_empty(),
        "unsafe continuation: {terminal}"
    );
}

#[test]
fn aggregated_repeated_cursor_does_not_offer_the_rejected_cursor_again() {
    assert_repeated_cursor_stops_without_a_continuation(false);
}

#[test]
fn ndjson_repeated_cursor_does_not_offer_the_rejected_cursor_again() {
    assert_repeated_cursor_stops_without_a_continuation(true);
}
