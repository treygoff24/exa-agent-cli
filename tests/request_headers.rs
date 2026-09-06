//! Wire and preview parity for shared request headers.

use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpListener, TcpStream};
use std::process::{Command, Output};
use std::thread;
use std::time::Duration;

/// A loopback server that answers every request with an empty result set. It serves in a loop and
/// stays bound until `ServerHandle::first_request` stops it, and it drains each socket before
/// closing: closing with bytes still queued makes Linux send RST instead of FIN, which throws away
/// the response the client has not read yet and surfaces as an intermittent `network_error`.
fn server() -> (String, ServerHandle) {
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let handle = thread::spawn(move || {
        let mut requests = Vec::new();
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let Some(request) = read_request(&mut stream) else {
                continue;
            };
            if request.starts_with("STOP") {
                break;
            }
            let body = br#"{"results":[]}"#;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
            stream.flush().unwrap();
            // Closing with unread bytes still queued makes Linux send RST instead of FIN, which
            // discards the response the client has not read yet. Half-close, then drain.
            let _ = stream.shutdown(Shutdown::Write);
            let _ = stream.read_to_end(&mut Vec::new());
            requests.push(request);
        }
        requests
    });
    (
        format!("http://{address}"),
        ServerHandle { address, handle },
    )
}

struct ServerHandle {
    address: SocketAddr,
    handle: thread::JoinHandle<Vec<String>>,
}

impl ServerHandle {
    /// Stop the server and return the first request it served.
    fn first_request(self) -> String {
        let mut stop = TcpStream::connect(self.address).expect("stop the loopback server");
        stop.write_all(b"STOP / HTTP/1.1\r\n\r\n").unwrap();
        stop.flush().unwrap();
        let requests = self.handle.join().expect("loopback server");
        requests
            .into_iter()
            .next()
            .expect("the loopback server received no request")
    }
}

/// Read one HTTP request, or `None` when the peer closed without sending a complete one.
fn read_request(stream: &mut std::net::TcpStream) -> Option<String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut request = Vec::new();
    let header_end = loop {
        let mut chunk = [0; 1024];
        let count = stream.read(&mut chunk).ok()?;
        if count == 0 {
            return None;
        }
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
        let count = stream.read(&mut chunk).ok()?;
        if count == 0 {
            return None;
        }
        request.extend_from_slice(&chunk[..count]);
    }
    Some(String::from_utf8(request).unwrap())
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
    let wire = wire_headers(&handle.first_request());
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
    let wire = wire_headers(&handle.first_request());
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

#[test]
fn session_credentials_cannot_be_echoed_through_custom_headers() {
    for name in ["Cookie", "Set-Cookie", "X-Session", "X-Session-Id"] {
        let header = format!("{name}: private-session-value");
        let output = command(&["search", "q", "--header", &header, "--dry-run"]);
        assert_eq!(output.status.code(), Some(1), "{name}");
        assert!(output.stdout.is_empty(), "{name}");
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["error"]["code"], "invalid_flag_combination");
        assert!(!String::from_utf8_lossy(&output.stderr).contains("private-session-value"));
    }
}

/// A required beta token the CLI injects must not be repeated when the caller already sent it by
/// hand: `Exa-Beta` is a single-value enum upstream, so `token,token` is a request a strict
/// gateway can reject.
#[test]
fn injected_beta_token_is_not_repeated_when_the_user_supplied_it() {
    let (base_url, handle) = server();
    let live = command(&[
        "batches",
        "list",
        "--base-url",
        &base_url,
        "--api-key",
        "api-secret",
        "--header",
        "Exa-Beta: batches-2026-06-06",
    ]);
    let request = handle.first_request();
    assert_eq!(live.status.code(), Some(0));
    assert_eq!(
        request
            .lines()
            .filter(|line| line.to_ascii_lowercase().starts_with("exa-beta:"))
            .count(),
        1,
        "exactly one Exa-Beta header:\n{request}"
    );
    assert_eq!(wire_headers(&request)["exa-beta"], "batches-2026-06-06");
}

/// Repeated `--header Exa-Beta:` lines and `--beta` all describe one wire header. They fold into a
/// single header whose tokens are unique and ordered header-first.
#[test]
fn repeated_beta_headers_collapse_into_one_deduplicated_header() {
    let (base_url, handle) = server();
    let live = command(&[
        "search",
        "q",
        "--base-url",
        &base_url,
        "--api-key",
        "api-secret",
        "--header",
        "Exa-Beta: alpha",
        "--header",
        "exa-beta: bravo, alpha",
        "--beta",
        "charlie",
    ]);
    let request = handle.first_request();
    assert_eq!(live.status.code(), Some(0));
    assert_eq!(
        request
            .lines()
            .filter(|line| line.to_ascii_lowercase().starts_with("exa-beta:"))
            .count(),
        1,
        "exactly one Exa-Beta header:\n{request}"
    );
    assert_eq!(wire_headers(&request)["exa-beta"], "alpha,bravo,charlie");
}

/// The `effort: max` gate reads the same two sources the wire header is built from, so opting in
/// through `--header` is accepted without also passing `--beta`.
#[test]
fn effort_max_accepts_the_beta_opt_in_supplied_through_a_header() {
    let preview = command(&[
        "agent",
        "runs",
        "create",
        "map the market",
        "--effort",
        "max",
        "--max-cost-dollars",
        "5",
        "--header",
        "Exa-Beta: agent-max-effort-2026-07-27",
        "--api-key",
        "api-secret",
        "--dry-run",
        "--print-request",
    ]);
    let headers = preview_headers(&json(preview));
    assert_eq!(headers["exa-beta"], "agent-max-effort-2026-07-27");
}
