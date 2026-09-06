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
    serde_json::from_slice(&output.stdout).unwrap()
}

fn server(response: Value) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let handle = std::thread::spawn(move || {
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
        String::from_utf8(request).unwrap()
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
