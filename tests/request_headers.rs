//! Wire and preview parity for shared request headers.

use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::{Command, Output};
use std::thread;
use std::time::Duration;

fn server() -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let header_end = loop {
            let mut chunk = [0; 1024];
            let count = stream.read(&mut chunk).unwrap();
            assert!(count > 0, "client closed before request headers");
            request.extend_from_slice(&chunk[..count]);
            if let Some(end) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                break end + 4;
            }
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.strip_prefix("Content-Length:")?
                    .trim()
                    .parse::<usize>()
                    .ok()
            })
            .unwrap_or(0);
        while request.len() < header_end + content_length {
            let mut chunk = [0; 1024];
            let count = stream.read(&mut chunk).unwrap();
            assert!(count > 0, "client closed before request body");
            request.extend_from_slice(&chunk[..count]);
        }
        let body = br#"{"results":[]}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .unwrap();
        stream.write_all(body).unwrap();
        String::from_utf8(request).unwrap()
    });
    (address, handle)
}

fn command(args: &[&str]) -> Output {
    let isolated = std::env::temp_dir().join(format!("exa-request-headers-{}", std::process::id()));
    let mut command = Command::new(env!("CARGO_BIN_EXE_exa-agent"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("EXA_") {
            command.env_remove(key);
        }
    }
    command
        .args(args)
        .arg("--json")
        .env("EXA_AGENT_CONFIG", isolated.join("config.toml"))
        .env("EXA_AGENT_CREDENTIALS", isolated.join("credentials.json"))
        .env("EXA_AGENT_STATE", isolated.join("state"))
        .output()
        .unwrap()
}

fn json(output: Output) -> Value {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn wire_headers(request: &str) -> BTreeMap<String, String> {
    request
        .split_once("\r\n\r\n")
        .unwrap()
        .0
        .lines()
        .skip(1)
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.to_ascii_lowercase(), value.trim().to_string()))
        })
        .collect()
}

fn preview_headers(response: &Value) -> BTreeMap<String, String> {
    response["data"]["request"]["headers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|header| {
            (
                header["name"].as_str().unwrap().to_ascii_lowercase(),
                header["value"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn search_wire_and_dry_run_share_user_beta_and_idempotency_headers() {
    let (base_url, handle) = server();
    let live = command(&[
        "search",
        "q",
        "--base-url",
        &base_url,
        "--api-key",
        "api-secret",
        "--header",
        "X-Caller: kept",
        "--beta",
        "search-beta",
        "--idempotency-key",
        "idem-request-1",
    ]);
    let wire = wire_headers(&handle.join().unwrap());
    assert_eq!(live.status.code(), Some(0));
    assert_eq!(wire["x-caller"], "kept");
    assert_eq!(wire["exa-beta"], "search-beta");
    assert_eq!(wire["idempotency-key"], "idem-request-1");
    assert_eq!(wire["x-api-key"], "api-secret");

    let preview_output = command(&[
        "search",
        "q",
        "--base-url",
        &base_url,
        "--api-key",
        "api-secret",
        "--header",
        "X-Caller: kept",
        "--beta",
        "search-beta",
        "--idempotency-key",
        "idem-request-1",
        "--dry-run",
        "--print-request",
    ]);
    let preview_text = String::from_utf8_lossy(&preview_output.stdout);
    assert!(!preview_text.contains("api-secret"));
    let preview = json(preview_output);
    let headers = preview_headers(&preview);
    assert_eq!(headers["x-caller"], "kept");
    assert_eq!(headers["exa-beta"], "search-beta");
    assert_eq!(headers["idempotency-key"], "idem-request-1");
}

#[test]
fn raw_wire_and_dry_run_share_headers_and_accept_respects_user_override() {
    let (base_url, handle) = server();
    let live = command(&[
        "raw",
        "POST",
        "/search",
        "--base-url",
        &base_url,
        "--api-key",
        "api-secret",
        "--body",
        r#"{"query":"q"}"#,
        "--header",
        "X-Caller: kept",
        "--beta",
        "raw-beta",
        "--idempotency-key",
        "idem-request-1",
    ]);
    assert_eq!(live.status.code(), Some(0));
    let wire = wire_headers(&handle.join().unwrap());
    assert_eq!(wire["x-caller"], "kept");
    assert_eq!(wire["exa-beta"], "raw-beta");
    assert_eq!(wire["idempotency-key"], "idem-request-1");

    let preview = json(command(&[
        "raw",
        "POST",
        "/search",
        "--base-url",
        &base_url,
        "--api-key",
        "api-secret",
        "--body",
        r#"{"query":"q"}"#,
        "--header",
        "X-Caller: kept",
        "--beta",
        "raw-beta",
        "--idempotency-key",
        "idem-request-1",
        "--dry-run",
        "--print-request",
    ]));
    let headers = preview_headers(&preview);
    assert_eq!(headers["x-caller"], "kept");
    assert_eq!(headers["exa-beta"], "raw-beta");
    assert_eq!(headers["idempotency-key"], "idem-request-1");

    let explicit = json(command(&[
        "search",
        "q",
        "--stream",
        "--header",
        "Accept: application/json",
        "--dry-run",
        "--print-request",
    ]));
    assert_eq!(preview_headers(&explicit)["accept"], "application/json");
    let synthesized = json(command(&[
        "search",
        "q",
        "--stream",
        "--dry-run",
        "--print-request",
    ]));
    assert_eq!(preview_headers(&synthesized)["accept"], "text/event-stream");
}

#[test]
fn chunked_contents_dry_run_includes_shared_headers() {
    let preview = json(command(&[
        "contents",
        "https://example.com",
        "--chunk-size",
        "10",
        "--header",
        "X-Caller: kept",
        "--beta",
        "contents-beta",
        "--idempotency-key",
        "idem-request-1",
        "--dry-run",
        "--print-request",
    ]));
    let headers = preview_headers(&preview);
    assert_eq!(headers["x-caller"], "kept");
    assert_eq!(headers["exa-beta"], "contents-beta");
    assert_eq!(headers["idempotency-key"], "idem-request-1");
}
