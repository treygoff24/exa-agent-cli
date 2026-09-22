use serde_json::Value;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

enum Reply {
    Json(&'static str),
    Sse(&'static str),
    Drop,
}

fn run(args: &[&str]) -> Output {
    run_with_env(args, &[])
}

fn run_owned(args: &[String]) -> Output {
    run(&args.iter().map(String::as_str).collect::<Vec<_>>())
}

fn run_with_env(args: &[&str], env: &[(&str, &str)]) -> Output {
    let hermetic = temp_path("hermetic");
    let mut command = Command::new(env!("CARGO_BIN_EXE_exa-agent"));
    command
        .args(args)
        .env_remove("EXA_AGENT_NO_NETWORK")
        .env_remove("EXA_API_KEY")
        .env_remove("EXA_SERVICE_KEY")
        .env_remove("EXA_PROFILE")
        .env_remove("EXA_OUTPUT")
        .env("EXA_AGENT_CONFIG", hermetic.join("config.toml"))
        .env("EXA_AGENT_CREDENTIALS", hermetic.join("credentials.json"));
    for (name, value) in env {
        command.env(name, value);
    }
    command
        .output()
        .unwrap_or_else(|err| panic!("failed to run exa-agent {args:?}: {err}"))
}

fn temp_path(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "exa-agent-async-{label}-{}-{stamp}",
        std::process::id()
    ))
}

/// The temp file the paginated writer stages pages in is an implementation detail; if one
/// survives a run, the writer leaked it.
fn directory_entries(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(directory)
        .expect("output directory")
        .map(|entry| entry.expect("directory entry").file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
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
    serde_json::from_slice(&output.stderr).unwrap_or_else(|err| {
        panic!(
            "stderr was not JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn ndjson(bytes: &[u8]) -> Vec<Value> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(|line| serde_json::from_str(line).expect("NDJSON line"))
        .collect()
}

/// Cap on the bytes one framed request may occupy, header section and declared body together. The
/// client's own requests are far smaller, so anything larger is a framing mistake or a probe to
/// reject rather than something to buffer.
const MAX_REQUEST_BYTES: usize = 64 * 1024;

/// Longest a single `read` may block before re-checking the absolute listener budget. It bounds
/// responsiveness only; the deadline is what bounds total time.
const READ_POLL: Duration = Duration::from_millis(200);

/// One HTTP request split at its message boundaries.
///
/// Expectations read these regions separately: a header-shaped line inside the framed body is body
/// text, and a body expectation is never satisfied by anything outside the framed body.
struct TestRequest {
    request_line: String,
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    /// Whether this request's own header section carried the test's key, decided from the raw
    /// header-section bytes so it does not depend on the structural parse succeeding.
    attributed: bool,
}

impl TestRequest {
    /// Header lookup over the header section only, by lowercased field name.
    fn header(&self, name: &str) -> Option<&str> {
        header_from(&self.headers, name)
    }

    fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    /// A diagnostic view. Nothing in the fixture matches against this rendering.
    fn render(&self) -> String {
        let mut text = self.request_line.clone();
        for (name, value) in &self.headers {
            text.push_str(&format!("\n{name}: {value}"));
        }
        text.push_str("\n\n");
        text.push_str(&self.body_text());
        text
    }
}

fn header_from<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(field, _)| field == name)
        .map(|(_, value)| value.as_str())
}

/// Attribution is read straight from the header-section bytes, so it survives a section the
/// structural parse cannot read: a malformed line, a non-UTF-8 byte, or an unframed head cannot
/// hide the key that marks the test's own request. Only complete field lines of the header section
/// are considered, the request line is skipped, and the framed body is never scanned. Two
/// conflicting copies of the key header attribute nothing: a request whose ownership is ambiguous
/// is foreign traffic, never this test's request.
fn attributed_header_bytes(header_section: &[u8]) -> bool {
    let mut lines = header_section.split(|byte| *byte == b'\n');
    lines.next(); // the request line
    let mut keyed: Option<bool> = None;
    for line in lines {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.is_empty() {
            break; // the empty line closes the header section
        }
        let Some(colon) = line.iter().position(|byte| *byte == b':') else {
            continue; // a line without a colon is not a field
        };
        let (name, value) = line.split_at(colon);
        if !name.trim_ascii().eq_ignore_ascii_case(b"x-api-key") {
            continue;
        }
        let matches = value[1..].trim_ascii() == TEST_API_KEY.as_bytes();
        if keyed.is_some_and(|seen| seen != matches) {
            return false;
        }
        keyed = Some(matches);
    }
    keyed.unwrap_or(false)
}

/// Why a connection could not be read as one framed request, and whether the header-section bytes
/// already read carry the test's own key.
struct Unreadable {
    /// Decided from the header-section bytes read before the failure, never from any body.
    attributed: bool,
    reason: &'static str,
}

impl Unreadable {
    fn new(header_section: &[u8], reason: &'static str) -> Self {
        Self {
            attributed: attributed_header_bytes(header_section),
            reason,
        }
    }
}

/// The request line and header section, before the framed body is read.
struct Head {
    request_line: String,
    method: String,
    target: String,
    headers: Vec<(String, String)>,
}

fn header_terminator(bytes: &[u8]) -> Option<usize> {
    bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|start| start + 4)
}

/// RFC 9110 token bytes: the only bytes an HTTP field name may contain.
fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

/// Splits a framed request head into its request line and headers. Attribution never depends on
/// this parse: it is decided from the raw header-section bytes.
fn parse_head(head: &str) -> Result<Head, &'static str> {
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or_default().to_string();
    let mut headers = Vec::new();
    for line in lines {
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            return Err("a header line carried no colon");
        };
        let name = name.trim().to_ascii_lowercase();
        let value = value.trim();
        if name.is_empty() || !name.bytes().all(is_token_byte) {
            return Err("a header name was not an HTTP token");
        }
        if value.bytes().any(|byte| byte < 0x20 && byte != b'\t') {
            return Err("a header value carried a control byte");
        }
        headers.push((name, value.to_string()));
    }
    let mut parts = request_line.split(' ');
    let (method, target) = match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(method), Some(target), Some(version), None)
            if !method.is_empty() && !target.is_empty() && version.starts_with("HTTP/") =>
        {
            (method, target)
        }
        _ => return Err("the request line was not `METHOD TARGET HTTP/x.y`"),
    };
    let method = method.to_string();
    let target = target.to_string();
    Ok(Head {
        request_line,
        method,
        target,
        headers,
    })
}

/// The declared body length, validated rather than assumed: an absent length on a method that
/// carries a body, a non-numeric value, conflicting copies, an unsupported framing, or a declared
/// body beyond the cap are all framing errors — never a zero-length body.
fn declared_body_length(headers: &[(String, String)], method: &str) -> Result<usize, &'static str> {
    if headers.iter().any(|(name, _)| name == "transfer-encoding") {
        return Err("the request was framed with Transfer-Encoding");
    }
    let lengths: Vec<&str> = headers
        .iter()
        .filter(|(name, _)| name == "content-length")
        .map(|(_, value)| value.as_str())
        .collect();
    let Some(declared) = lengths.first() else {
        return if method == "POST" {
            Err("a POST arrived without Content-Length")
        } else {
            Ok(0)
        };
    };
    if lengths.len() > 1 {
        return Err("the request carried more than one Content-Length");
    }
    let length = declared
        .parse::<usize>()
        .map_err(|_| "Content-Length was not a number")?;
    if length > MAX_REQUEST_BYTES {
        return Err("the declared body exceeded the request byte cap");
    }
    Ok(length)
}

enum ReadStep {
    Data,
    Closed,
    Idle,
}

/// One bounded read, never taking more than `limit` bytes, so a caller that has already checked its
/// remaining capacity cannot have that capacity exceeded by the read itself. Callers only pass a
/// positive `limit`; a zero limit reads nothing. The read timeout is the smaller of the remaining
/// budget and `READ_POLL`, so a trickle of bytes can never extend the absolute budget.
fn read_step(
    stream: &mut TcpStream,
    bytes: &mut Vec<u8>,
    limit: usize,
    deadline: Instant,
) -> ReadStep {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() || limit == 0 {
        return ReadStep::Idle;
    }
    stream
        .set_read_timeout(Some(remaining.min(READ_POLL)))
        .unwrap();
    let mut buffer = [0u8; 4096];
    let take = limit.min(buffer.len());
    let chunk = &mut buffer[..take];
    match stream.read(chunk) {
        Ok(0) => ReadStep::Closed,
        Ok(n) => {
            bytes.extend_from_slice(&chunk[..n]);
            ReadStep::Data
        }
        Err(err) if matches!(err.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {
            ReadStep::Idle
        }
        Err(err)
            if matches!(
                err.kind(),
                ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted
            ) =>
        {
            ReadStep::Closed
        }
        Err(err) => panic!("read request: {err}"),
    }
}

/// Reads exactly one framed request: a request line, a header section, and the body the header
/// section declares. `deadline` is the listener's absolute budget and covers this read too, so no
/// amount of slow or trickled input can extend it. `MAX_REQUEST_BYTES` bounds the whole framed
/// request: every read takes no more than the capacity left, and a header section plus declared
/// body that would together exceed the cap is refused before any body byte is read.
fn read_request(stream: &mut TcpStream, deadline: Instant) -> Result<TestRequest, Unreadable> {
    let mut bytes = Vec::new();
    let header_end = loop {
        if let Some(end) = header_terminator(&bytes) {
            break end;
        }
        // Without a terminator every byte read so far is still header-section bytes, so a head that
        // would outgrow the cap is refused here rather than framed past it.
        if bytes.len() >= MAX_REQUEST_BYTES {
            return Err(Unreadable::new(
                &bytes,
                "the header section exceeded the request byte cap",
            ));
        }
        if Instant::now() >= deadline {
            return Err(Unreadable::new(
                &bytes,
                "the header section did not arrive within the listener budget",
            ));
        }
        let capacity = MAX_REQUEST_BYTES - bytes.len();
        if let ReadStep::Closed = read_step(stream, &mut bytes, capacity, deadline) {
            return Err(Unreadable::new(
                &bytes,
                "the connection closed before the header section was complete",
            ));
        }
    };
    let head_text = match std::str::from_utf8(&bytes[..header_end]) {
        Ok(text) => text,
        Err(_) => {
            return Err(Unreadable::new(
                &bytes[..header_end],
                "the header section was not UTF-8",
            ))
        }
    };
    let head =
        parse_head(head_text).map_err(|reason| Unreadable::new(&bytes[..header_end], reason))?;
    let declared = declared_body_length(&head.headers, &head.method)
        .map_err(|reason| Unreadable::new(&bytes[..header_end], reason))?;
    let framed_end = header_end
        .checked_add(declared)
        .filter(|end| *end <= MAX_REQUEST_BYTES)
        .ok_or_else(|| {
            Unreadable::new(
                &bytes[..header_end],
                "the framed request exceeded the request byte cap",
            )
        })?;
    while bytes.len() < framed_end {
        if Instant::now() >= deadline {
            return Err(Unreadable::new(
                &bytes[..header_end],
                "the request body did not arrive within the listener budget",
            ));
        }
        let capacity = framed_end - bytes.len();
        if let ReadStep::Closed = read_step(stream, &mut bytes, capacity, deadline) {
            return Err(Unreadable::new(
                &bytes[..header_end],
                "the connection closed before the declared body arrived",
            ));
        }
    }
    Ok(TestRequest {
        request_line: head.request_line,
        method: head.method,
        target: head.target,
        headers: head.headers,
        body: bytes[header_end..framed_end].to_vec(),
        attributed: attributed_header_bytes(&bytes[..header_end]),
    })
}

/// A raw client for the fixture, used to send byte sequences the real CLI would never send so the
/// listener's attribution, framing, and budget can be exercised directly.
struct Probe {
    stream: TcpStream,
    received: Vec<u8>,
}

impl Probe {
    fn connect(base_url: &str) -> Self {
        let address = base_url.strip_prefix("http://").expect("fixture base URL");
        Self {
            stream: TcpStream::connect(address).expect("probe connect"),
            received: Vec::new(),
        }
    }

    fn send(&mut self, bytes: &[u8]) {
        self.stream.write_all(bytes).expect("probe write");
    }

    /// Reads until the peer closes or `timeout` elapses; returns the bytes received by this call.
    fn receive(&mut self, timeout: Duration) -> Vec<u8> {
        self.stream.set_read_timeout(Some(timeout)).unwrap();
        let start = self.received.len();
        let mut chunk = [0u8; 4096];
        loop {
            match self.stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => self.received.extend_from_slice(&chunk[..n]),
                Err(err) if matches!(err.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => {
                    break
                }
                Err(err)
                    if matches!(
                        err.kind(),
                        ErrorKind::ConnectionReset | ErrorKind::ConnectionAborted
                    ) =>
                {
                    break
                }
                Err(err) => panic!("probe read: {err}"),
            }
        }
        self.received[start..].to_vec()
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.received).into_owned()
    }
}

/// The request the test's own client is expected to send. The mock listener serves nothing it
/// cannot attribute to the test, and never records a foreign connection as the test's request.
#[derive(Clone, Copy)]
struct ExpectedRequest {
    method: &'static str,
    path: &'static str,
    body: Option<BodyExpectation>,
}

/// What the framed body must hold. Pinning one decisive JSON field keeps the expectation from being
/// satisfied by an unrelated part of the body, which a substring search would allow.
#[derive(Clone, Copy)]
enum BodyExpectation {
    /// The framed body must be JSON whose value at this JSON pointer is exactly this string.
    JsonField {
        pointer: &'static str,
        value: &'static str,
    },
}

impl BodyExpectation {
    fn satisfied_by(&self, body: &[u8]) -> bool {
        let Self::JsonField { pointer, value } = self;
        serde_json::from_slice::<Value>(body)
            .ok()
            .and_then(|json| json.pointer(pointer).cloned())
            .is_some_and(|found| found == Value::String((*value).to_string()))
    }

    fn describe(&self) -> String {
        let Self::JsonField { pointer, value } = self;
        format!("JSON {pointer} = {value:?}")
    }
}

impl ExpectedRequest {
    const fn get(path: &'static str) -> Self {
        Self {
            method: "GET",
            path,
            body: None,
        }
    }

    const fn post(path: &'static str, pointer: &'static str, value: &'static str) -> Self {
        Self {
            method: "POST",
            path,
            body: Some(BodyExpectation::JsonField { pointer, value }),
        }
    }

    fn describe(&self) -> String {
        match &self.body {
            Some(body) => format!(
                "{} {} with body {}",
                self.method,
                self.path,
                body.describe()
            ),
            None => format!("{} {}", self.method, self.path),
        }
    }

    /// The request line must name this method and this exact path (a query string is allowed), and
    /// any pinned body expectation must hold for the framed body alone.
    fn matches(&self, request: &TestRequest) -> bool {
        let path_matches = request.target == self.path
            || request
                .target
                .strip_prefix(self.path)
                .is_some_and(|rest| rest.starts_with('?'));
        request.method == self.method
            && path_matches
            && self
                .body
                .as_ref()
                .is_none_or(|body| body.satisfied_by(&request.body))
    }
}

/// Every live call in this file authenticates with this key, so a connection without it cannot be
/// the test's own request.
const TEST_API_KEY: &str = "test-key-abcdef12";

/// Enough unrelated connections for a scanner sweep, few enough that a listener pointed at the wrong
/// place fails loudly instead of hanging.
const FOREIGN_PROBE_LIMIT: usize = 64;

/// Absolute budget for the whole fixture: accepting plus every read of every probe and expected
/// request. Tests that provoke slow input shorten it.
const LISTENER_BUDGET: Duration = Duration::from_secs(30);

/// The listener is non-blocking so accepts can honour `deadline`; the accepted stream is put back in
/// blocking mode because bounded reads need read timeouts.
fn accept_before(
    listener: &TcpListener,
    expected: &ExpectedRequest,
    deadline: Instant,
) -> TcpStream {
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(false)
                    .expect("blocking request stream");
                return stream;
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock && Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(err) if err.kind() == ErrorKind::WouldBlock => panic!(
                "timed out waiting for the request this test expects ({})",
                expected.describe()
            ),
            Err(err) => panic!("accept request: {err}"),
        }
    }
}

/// Turns one rejected connection away. Only the canned 404 goes out: probe bytes are never echoed,
/// and a rejected probe can never consume a canned reply.
fn reject(stream: &mut TcpStream) {
    let _ = stream
        .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let _ = stream.flush();
}

fn local_server(
    expected: ExpectedRequest,
    replies: Vec<Reply>,
) -> (String, thread::JoinHandle<Vec<TestRequest>>) {
    local_server_with_budget(expected, replies, LISTENER_BUDGET)
}

/// Serves `replies` to the test's own requests only. Any other connection is rejected and forgotten
/// instead of consuming a reply, so foreign, malformed, or body-spoofed traffic can neither fail
/// nor be mistaken for the test's request. A connection that does carry the test's key but cannot be
/// framed, or is not the expected shape, fails the fixture loudly before any reply is written.
fn local_server_with_budget(
    expected: ExpectedRequest,
    replies: Vec<Reply>,
    budget: Duration,
) -> (String, thread::JoinHandle<Vec<TestRequest>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let deadline = Instant::now() + budget;
        let mut requests = Vec::new();
        let mut rejected = Vec::new();
        for reply in replies {
            let (mut stream, request) = loop {
                let mut stream = accept_before(&listener, &expected, deadline);
                let label = match read_request(&mut stream, deadline) {
                    Ok(request) if request.attributed => break (stream, request),
                    Ok(request) => format!("{} {}", request.method, request.target),
                    Err(unreadable) if unreadable.attributed => panic!(
                        "the test's own client sent a request this fixture cannot frame ({}), expected {}",
                        unreadable.reason,
                        expected.describe()
                    ),
                    Err(unreadable) => format!("unparsed request ({})", unreadable.reason),
                };
                rejected.push(label);
                assert!(
                    rejected.len() <= FOREIGN_PROBE_LIMIT,
                    "unrelated connections reached this test's listener {rejected:?}, expected {}",
                    expected.describe()
                );
                reject(&mut stream);
            };
            assert!(
                expected.matches(&request),
                "this test's request did not match {}:\n{}",
                expected.describe(),
                request.render()
            );
            requests.push(request);
            let (content_type, body) = match reply {
                Reply::Json(body) => ("application/json", body),
                Reply::Sse(body) => ("text/event-stream", body),
                Reply::Drop => continue,
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
            stream.flush().unwrap();
        }
        requests
    });
    (format!("http://{address}"), server)
}

fn paginated_args(base_url: &str) -> Vec<String> {
    vec![
        "batches".into(),
        "list".into(),
        "--all".into(),
        "--limit".into(),
        "1".into(),
        "--ndjson".into(),
        "--retry".into(),
        "0".into(),
        "--base-url".into(),
        base_url.into(),
        "--api-key".into(),
        TEST_API_KEY.into(),
        "--compact".into(),
    ]
}

#[test]
fn all_ndjson_omits_intermediate_continuations_but_keeps_early_stop_recovery() {
    let (base_url, server) = local_server(
        ExpectedRequest::get("/batches"),
        vec![
            Reply::Json(r#"{"data":[{"id":"batch_1"}],"hasMore":true,"nextCursor":"cur2"}"#),
            Reply::Json(r#"{"data":[{"id":"batch_2"}],"hasMore":false,"nextCursor":null}"#),
        ],
    );
    let output = run_owned(&paginated_args(&base_url));
    let requests = server.join().expect("pagination server");
    assert_eq!(requests.len(), 2);
    let pages = ndjson(&output.stdout);
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0]["data"]["data"][0]["id"], "batch_1");
    assert_eq!(pages[1]["data"]["data"][0]["id"], "batch_2");
    assert_eq!(pages[0]["nextActions"], serde_json::json!([]));
    assert_eq!(pages[1]["nextActions"], serde_json::json!([]));

    let (base_url, server) = local_server(
        ExpectedRequest::get("/batches"),
        vec![Reply::Json(
            r#"{"data":[{"id":"batch_cap"}],"hasMore":true,"nextCursor":"resume"}"#,
        )],
    );
    let mut args = paginated_args(&base_url);
    args.extend(["--max-pages".into(), "1".into()]);
    let output = run_owned(&args);
    server.join().expect("max-pages server");
    let pages = ndjson(&output.stdout);
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0]["pagination"]["hasMore"], true);
    assert!(pages[0]["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["code"] == "pagination_max_pages_reached"));
    assert!(pages[0]["nextActions"][0]["command"]
        .as_str()
        .unwrap()
        .contains("--cursor=resume"));

    let (base_url, server) = local_server(
        ExpectedRequest::get("/batches"),
        vec![Reply::Json(
            r#"{"data":[{"id":"batch_repeat"}],"hasMore":true,"nextCursor":"same"}"#,
        )],
    );
    let mut args = paginated_args(&base_url);
    args.extend(["--cursor".into(), "same".into()]);
    let output = run_owned(&args);
    server.join().expect("repeated cursor server");
    let pages = ndjson(&output.stdout);
    assert_eq!(pages[0]["pagination"]["hasMore"], false);
    assert_eq!(pages[0]["nextActions"], serde_json::json!([]));
}

#[test]
fn all_ndjson_output_streams_pages_to_file_and_confirms_once() {
    let (base_url, server) = local_server(
        ExpectedRequest::get("/batches"),
        vec![
            Reply::Json(r#"{"data":[{"id":"batch_1"}],"hasMore":true,"nextCursor":"cur2"}"#),
            Reply::Json(r#"{"data":[{"id":"batch_2"}],"hasMore":false,"nextCursor":null}"#),
        ],
    );
    let output_path = temp_path("pages").join("pages.ndjson");
    fs::create_dir_all(output_path.parent().unwrap()).unwrap();
    let mut args = paginated_args(&base_url);
    args.extend([
        "--output".into(),
        output_path.to_string_lossy().into_owned(),
    ]);
    let output = run_owned(&args);
    server.join().expect("output pagination server");

    let confirmation = stdout_json(&output);
    assert_eq!(confirmation["command"], "batches list");
    assert_eq!(confirmation["data"], Value::Null);
    assert_eq!(
        confirmation["dataPath"],
        output_path.to_string_lossy().as_ref()
    );
    assert_eq!(confirmation["dataTruncated"], true);
    let bytes = fs::read(&output_path).expect("NDJSON output file");
    assert_eq!(confirmation["bytes"], bytes.len() as u64);
    let pages = ndjson(&bytes);
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0]["data"]["data"][0]["id"], "batch_1");
    assert_eq!(pages[1]["data"]["data"][0]["id"], "batch_2");
    assert_eq!(pages[0]["nextActions"], serde_json::json!([]));
    assert_eq!(
        directory_entries(output_path.parent().unwrap()),
        vec!["pages.ndjson".to_string()]
    );
}

#[test]
fn first_page_failure_creates_neither_output_file_nor_temp() {
    let directory = temp_path("absent-pages");
    fs::create_dir_all(&directory).unwrap();
    let output_path = directory.join("pages.ndjson");
    let (base_url, server) = local_server(ExpectedRequest::get("/batches"), vec![Reply::Drop]);
    let mut args = paginated_args(&base_url);
    args.extend([
        "--output".into(),
        output_path.to_string_lossy().into_owned(),
    ]);
    let output = run_owned(&args);
    server.join().expect("first page failure server");
    assert!(!output.status.success());
    assert_eq!(directory_entries(&directory), Vec::<String>::new());
    let details = &stderr_json(&output)["error"]["details"];
    assert_eq!(details["outputPartial"], false);
    assert_eq!(details["outputPages"], 0);
    assert_eq!(details["outputBytes"], 0);
}

#[test]
fn first_page_failure_preserves_existing_output_and_capped_success_exposes_recovery() {
    let output_path = temp_path("existing-pages").join("pages.ndjson");
    fs::create_dir_all(output_path.parent().unwrap()).unwrap();
    fs::write(&output_path, "previous result\n").unwrap();
    let (base_url, server) = local_server(ExpectedRequest::get("/batches"), vec![Reply::Drop]);
    let mut args = paginated_args(&base_url);
    args.extend([
        "--output".into(),
        output_path.to_string_lossy().into_owned(),
    ]);
    let output = run_owned(&args);
    server.join().unwrap();
    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(&output_path).unwrap(),
        "previous result\n"
    );
    assert_eq!(
        directory_entries(output_path.parent().unwrap()),
        vec!["pages.ndjson".to_string()]
    );
    // The pre-existing file is not ours to claim credit for.
    assert_eq!(stderr_json(&output)["error"]["details"]["outputBytes"], 0);

    let (base_url, server) = local_server(
        ExpectedRequest::get("/batches"),
        vec![Reply::Json(
            r#"{"data":[{"id":"new-result"}],"hasMore":true,"nextCursor":"next-page"}"#,
        )],
    );
    let mut args = paginated_args(&base_url);
    args.extend([
        "--output".into(),
        output_path.to_string_lossy().into_owned(),
        "--max-pages".into(),
        "1".into(),
    ]);
    let output = run_owned(&args);
    server.join().unwrap();
    let confirmation = stdout_json(&output);
    assert_eq!(confirmation["pagination"]["nextCursor"], "next-page");
    assert!(!confirmation["nextActions"].as_array().unwrap().is_empty());
    let pages = ndjson(&fs::read(&output_path).unwrap());
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0]["data"]["data"][0]["id"], "new-result");
    assert_eq!(confirmation["nextActions"], pages[0]["nextActions"]);
    assert!(pages[0]["nextActions"][0]["command"]
        .as_str()
        .unwrap()
        .contains("--cursor=next-page"));
    assert_eq!(
        directory_entries(output_path.parent().unwrap()),
        vec!["pages.ndjson".to_string()]
    );
}

#[test]
fn all_ndjson_output_open_failure_stops_before_network() {
    let output_path = temp_path("missing-parent").join("pages.ndjson");
    let output = run_owned(&[
        "batches".into(),
        "list".into(),
        "--all".into(),
        "--ndjson".into(),
        "--output".into(),
        output_path.to_string_lossy().into_owned(),
        "--retry".into(),
        "0".into(),
        "--base-url".into(),
        "http://127.0.0.1:1".into(),
        "--api-key".into(),
        TEST_API_KEY.into(),
        "--compact".into(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let error = stderr_json(&output);
    assert_eq!(error["error"]["code"], "invalid_value");
    assert!(error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("--output"));
    assert!(!output_path.exists());
}

#[test]
fn all_ndjson_output_discloses_saved_pages_when_later_request_fails() {
    let (base_url, server) = local_server(
        ExpectedRequest::get("/batches"),
        vec![
            Reply::Json(r#"{"data":[{"id":"saved"}],"hasMore":true,"nextCursor":"cur2"}"#),
            Reply::Drop,
        ],
    );
    let output_path = temp_path("partial").join("pages.ndjson");
    fs::create_dir_all(output_path.parent().unwrap()).unwrap();
    let pending_path = temp_path("partial-pending").join("pending.jsonl");
    let mut args = paginated_args(&base_url);
    args.extend([
        "--output".into(),
        output_path.to_string_lossy().into_owned(),
    ]);
    let output = run_with_env(
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        &[(
            "EXA_AGENT_PENDING_RUNS",
            pending_path.to_string_lossy().as_ref(),
        )],
    );
    server.join().expect("partial pagination server");

    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let pages = ndjson(&fs::read(&output_path).expect("partial output survives"));
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0]["data"]["data"][0]["id"], "saved");
    let error = stderr_json(&output);
    let details = &error["error"]["details"];
    assert_eq!(
        details["outputPath"],
        output_path.to_string_lossy().as_ref()
    );
    assert_eq!(details["outputPartial"], true);
    assert_eq!(details["outputPages"], 1);
    assert_eq!(details["resumeCursor"], "cur2");
    assert!(details["outputBytes"].as_u64().unwrap() > 0);
    assert_eq!(
        directory_entries(output_path.parent().unwrap()),
        vec!["pages.ndjson".to_string()]
    );
}

#[cfg(target_os = "linux")]
#[test]
fn all_ndjson_output_write_failure_is_not_reported_as_success() {
    let (base_url, server) = local_server(
        ExpectedRequest::get("/batches"),
        vec![Reply::Json(
            r#"{"data":[{"id":"never-saved"}],"hasMore":false,"nextCursor":null}"#,
        )],
    );
    let mut args = paginated_args(&base_url);
    args.extend(["--output".into(), "/dev/full".into()]);
    let output = run_owned(&args);
    server.join().expect("write-failure server");
    assert_eq!(output.status.code(), Some(1));
    let recovered = ndjson(&output.stdout);
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0]["data"]["data"][0]["id"], "never-saved");
    assert!(recovered[0]["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning["code"] == "output_write_failed"));
    assert!(recovered[0].get("dataPath").is_none());
    let error = stderr_json(&output);
    assert_eq!(error["error"]["details"]["outputPath"], "/dev/full");
    assert_eq!(error["error"]["details"]["outputPartial"], false);
    assert_eq!(error["error"]["details"]["outputPages"], 0);
}

#[test]
fn agent_create_stream_adds_followups_only_for_final_completed_event() {
    let complete = r#"id: evt-created
event: agent_run.created
data: {"id":"agent_run_initial","status":"running"}

id: evt-completed
event: agent_run.completed
data: {"id":"agent_run_final","status":"completed","output":{"text":"done"}}

data: [DONE]

"#;
    let (base_url, server) = local_server(
        ExpectedRequest::post("/agent/runs", "/query", "finish the research"),
        vec![Reply::Sse(complete)],
    );
    let output = run_owned(&[
        "agent".into(),
        "runs".into(),
        "create".into(),
        "finish the research".into(),
        "--stream".into(),
        "--json".into(),
        "--retry".into(),
        "0".into(),
        "--base-url".into(),
        base_url,
        "--api-key".into(),
        TEST_API_KEY.into(),
        "--compact".into(),
    ]);
    server.join().expect("agent stream server");
    let terminal = stdout_json(&output);
    let actions = terminal["nextActions"].as_array().unwrap();
    // Only "Inspect the created resource": a run that already reported `completed` has no
    // remaining events, so `agent runs events --stream` would be dead advice.
    assert_eq!(actions.len(), 1, "{actions:?}");
    assert_eq!(actions[0]["description"], "Inspect the created resource");
    let command = actions[0]["command"].as_str().unwrap();
    assert!(command.contains("agent_run_final"), "{command}");
    assert!(!command.contains("agent_run_initial"), "{command}");
    assert!(!command.contains("--stream"), "{command}");

    // Upstream may emit a `usage` event after the terminal one. Selecting the last event that
    // merely carries an id (rather than the last completed one) silenced the follow-up.
    let trailing_usage = r#"id: evt-created
event: agent_run.created
data: {"id":"agent_run_initial","status":"running"}

id: evt-completed
event: agent_run.completed
data: {"id":"agent_run_final","status":"completed","output":{"text":"done"}}

id: evt-usage
event: agent_run.usage
data: {"id":"usage_1","tokens":42}

data: [DONE]

"#;
    let (base_url, server) = local_server(
        ExpectedRequest::post("/agent/runs", "/query", "finish the research"),
        vec![Reply::Sse(trailing_usage)],
    );
    let output = run_owned(&[
        "agent".into(),
        "runs".into(),
        "create".into(),
        "finish the research".into(),
        "--stream".into(),
        "--json".into(),
        "--retry".into(),
        "0".into(),
        "--base-url".into(),
        base_url,
        "--api-key".into(),
        TEST_API_KEY.into(),
        "--compact".into(),
    ]);
    server.join().expect("trailing usage stream server");
    let terminal = stdout_json(&output);
    let actions = terminal["nextActions"].as_array().unwrap();
    assert_eq!(actions.len(), 1, "{actions:?}");
    let command = actions[0]["command"].as_str().unwrap();
    assert!(command.contains("agent_run_final"), "{command}");
    assert!(!command.contains("usage_1"), "{command}");

    let incomplete = r#"id: evt-running
event: agent_run.running
data: {"id":"agent_run_running","status":"running"}

data: [DONE]

"#;
    let (base_url, server) = local_server(
        ExpectedRequest::post("/agent/runs", "/query", "unfinished research"),
        vec![Reply::Sse(incomplete)],
    );
    let output = run_owned(&[
        "agent".into(),
        "runs".into(),
        "create".into(),
        "unfinished research".into(),
        "--stream".into(),
        "--json".into(),
        "--retry".into(),
        "0".into(),
        "--base-url".into(),
        base_url,
        "--api-key".into(),
        TEST_API_KEY.into(),
        "--compact".into(),
    ]);
    server.join().expect("incomplete agent stream server");
    let terminal = stdout_json(&output);
    assert_eq!(terminal["nextActions"], serde_json::json!([]));
}

fn run_ambiguous_create(
    expected: ExpectedRequest,
    mut args: Vec<String>,
    extra_env: &[(&str, &str)],
) -> (Output, Vec<TestRequest>) {
    let (base_url, server) = local_server(expected, vec![Reply::Drop]);
    args.extend([
        "--retry".into(),
        "0".into(),
        "--base-url".into(),
        base_url,
        "--api-key".into(),
        TEST_API_KEY.into(),
        "--compact".into(),
    ]);
    let output = run_with_env(
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        extra_env,
    );
    let requests = server.join().expect("ambiguous create server");
    (output, requests)
}

fn pending_record(path: &Path) -> Value {
    let text = fs::read_to_string(path).expect("pending record");
    serde_json::from_str(text.lines().next().expect("pending line")).unwrap()
}

#[test]
fn ambiguous_recovery_preserves_safe_scope_but_omits_credentials_and_output() {
    let pending_path = temp_path("scoped-pending").join("pending.jsonl");
    fs::create_dir_all(pending_path.parent().unwrap()).unwrap();
    let output_path = temp_path("unused-output").join("response.json");
    let pending = pending_path.to_string_lossy().into_owned();
    let (output, requests) = run_ambiguous_create(
        ExpectedRequest::post("/agent/runs", "/query", "scoped recovery"),
        vec![
            "agent".into(),
            "runs".into(),
            "create".into(),
            "scoped recovery".into(),
            "--profile".into(),
            "ops".into(),
            "--beta".into(),
            "agent-test-beta".into(),
            "--output".into(),
            output_path.to_string_lossy().into_owned(),
        ],
        &[("EXA_AGENT_PENDING_RUNS", pending.as_str())],
    );
    assert_eq!(requests.len(), 1);
    assert!(!output.status.success());
    let error = stderr_json(&output);
    let suggestion = error["error"]["suggestedCommand"].as_str().unwrap();
    assert!(suggestion.contains("--profile=ops"), "{suggestion}");
    assert!(
        suggestion.contains("--base-url=http://127.0.0.1:"),
        "{suggestion}"
    );
    assert!(
        suggestion.contains("--beta=agent-test-beta"),
        "{suggestion}"
    );
    assert!(
        suggestion.ends_with("agent runs list --limit 10"),
        "{suggestion}"
    );
    assert!(!suggestion.contains("test-key"));
    assert!(!suggestion.contains("--output"));
    assert_eq!(error["error"]["details"]["recoveryContextRequired"], false);
    assert_eq!(pending_record(&pending_path)["recoveryCommand"], suggestion);
}

#[test]
fn ambiguous_recovery_with_private_header_requires_original_context() {
    let pending_path = temp_path("private-pending").join("pending.jsonl");
    fs::create_dir_all(pending_path.parent().unwrap()).unwrap();
    let pending = pending_path.to_string_lossy().into_owned();
    let (output, requests) = run_ambiguous_create(
        ExpectedRequest::post("/agent/runs", "/query", "private recovery"),
        vec![
            "agent".into(),
            "runs".into(),
            "create".into(),
            "private recovery".into(),
            "--header".into(),
            "X-Proxy-Context: opaque-routing-value".into(),
        ],
        &[("EXA_AGENT_PENDING_RUNS", pending.as_str())],
    );
    assert_eq!(requests.len(), 1);
    assert!(!output.status.success());
    let error = stderr_json(&output);
    assert_eq!(
        error["error"]["suggestedCommand"],
        "exa-agent agent runs create --help"
    );
    assert_eq!(error["error"]["details"]["recoveryContextRequired"], true);
    assert!(error["error"]["details"]["recoveryNote"]
        .as_str()
        .unwrap()
        .contains("original request context"));
    let rendered = String::from_utf8_lossy(&output.stderr);
    assert!(!rendered.contains("opaque-routing-value"));
    let record = pending_record(&pending_path);
    assert_eq!(
        record["recoveryCommand"],
        "exa-agent agent runs create --help"
    );
    assert!(!fs::read_to_string(&pending_path)
        .unwrap()
        .contains("opaque-routing-value"));
}

#[test]
fn keyed_batch_ambiguous_failure_still_records_list_recovery() {
    let pending_path = temp_path("keyed-batch-pending").join("pending.jsonl");
    fs::create_dir_all(pending_path.parent().unwrap()).unwrap();
    let pending = pending_path.to_string_lossy().into_owned();
    let (output, requests) = run_ambiguous_create(
        ExpectedRequest::post("/batches", "/requests/0/customId", "row-1"),
        vec![
            "batches".into(),
            "create".into(),
            "--requests".into(),
            r#"[{"customId":"row-1","method":"POST","url":"/search","body":{"query":"AI"}}]"#
                .into(),
            "--idempotency-key".into(),
            "batch-key-1".into(),
        ],
        &[("EXA_AGENT_PENDING_RUNS", pending.as_str())],
    );
    assert_eq!(requests.len(), 1);
    assert!(!output.status.success());
    let error = stderr_json(&output);
    assert_eq!(error["error"]["details"]["pendingRunWritten"], true);
    let suggestion = error["error"]["suggestedCommand"].as_str().unwrap();
    assert!(
        suggestion.ends_with("batches list --limit 10"),
        "{suggestion}"
    );
    assert!(!suggestion.contains("batch-key-1"));
    let record = pending_record(&pending_path);
    assert_eq!(record["command"], "batches create");
    assert_eq!(record["recoveryCommand"], suggestion);
}

/// The staging rename replaces the inode, so the file's own permissions and any symlink at the
/// requested path have to be carried over deliberately.
#[cfg(unix)]
#[test]
fn all_ndjson_output_keeps_file_mode_and_writes_through_symlink() {
    use std::os::unix::fs::PermissionsExt;
    let directory = temp_path("mode");
    fs::create_dir_all(&directory).unwrap();
    let target = directory.join("target.ndjson");
    fs::write(&target, b"stale\n").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    let link = directory.join("latest.ndjson");
    std::os::unix::fs::symlink(&target, &link).unwrap();

    let (base_url, server) = local_server(
        ExpectedRequest::get("/batches"),
        vec![
            Reply::Json(r#"{"data":[{"id":"batch_1"}],"hasMore":true,"nextCursor":"cur2"}"#),
            Reply::Json(r#"{"data":[{"id":"batch_2"}],"hasMore":false,"nextCursor":null}"#),
        ],
    );
    let mut args = paginated_args(&base_url);
    args.extend(["--output".into(), link.to_string_lossy().into_owned()]);
    let output = run_owned(&args);
    server.join().expect("output pagination server");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
    let meta = fs::metadata(&target).unwrap();
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    let pages = ndjson(&fs::read(&target).unwrap());
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[1]["data"]["data"][0]["id"], "batch_2");
    let mut entries = directory_entries(&directory);
    entries.sort();
    assert_eq!(
        entries,
        vec!["latest.ndjson".to_string(), "target.ndjson".to_string()]
    );
}

/// A staging file that already exists at the sibling name is never opened through: a planted
/// symlink there must not have its target truncated. The refusal happens before any request.
#[cfg(unix)]
#[test]
fn all_ndjson_output_refuses_to_follow_a_planted_staging_symlink() {
    let directory = temp_path("planted");
    fs::create_dir_all(&directory).unwrap();
    let victim = directory.join("victim.txt");
    fs::write(&victim, b"keep me\n").unwrap();
    let output_path = directory.join("pages.ndjson");
    // The staging name is `<output>.tmp-<pid>`; the child's pid is unknown ahead of time, so
    // plant links for a window of pids after our own and skip if the child landed outside it.
    let own = std::process::id();
    for pid in own..own + 4096 {
        let _ = std::os::unix::fs::symlink(&victim, format!("{}.tmp-{pid}", output_path.display()));
    }
    let output = run_owned(&[
        "batches".into(),
        "list".into(),
        "--all".into(),
        "--ndjson".into(),
        "--output".into(),
        output_path.to_string_lossy().into_owned(),
        "--retry".into(),
        "0".into(),
        "--base-url".into(),
        "http://127.0.0.1:1".into(),
        "--api-key".into(),
        TEST_API_KEY.into(),
        "--compact".into(),
    ]);
    let victim_bytes = fs::read(&victim).unwrap();
    let output_exists = output_path.exists();
    fs::remove_dir_all(&directory).unwrap();
    assert_eq!(victim_bytes, b"keep me\n");
    assert!(!output_exists);
    let error = stderr_json(&output);
    if error["error"]["code"] == "network_error" {
        eprintln!("child pid fell outside the planted window; the open succeeded");
        return;
    }
    assert_eq!(error["error"]["code"], "invalid_value", "{error}");
    assert!(error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("File exists"));
    assert_eq!(error["error"]["details"]["outputPartial"], false);
    assert_eq!(error["error"]["details"]["outputBytes"], 0);
}

/// A well-framed request carrying the test's own key, used to drive the fixture directly.
fn attributed_request(method: &str, target: &str, body: &str) -> Vec<u8> {
    format!(
        "{method} {target} HTTP/1.1\r\nhost: fixture\r\nx-api-key: {TEST_API_KEY}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

/// Asserts that a probe was turned away with only the canned rejection and no fixture reply.
fn assert_rejected(probe: &mut Probe, label: &str) {
    probe.receive(Duration::from_secs(10));
    let text = probe.text();
    assert!(
        text.starts_with("HTTP/1.1 404"),
        "{label} was not rejected: {text:?}"
    );
    assert!(
        !text.contains("200 OK"),
        "{label} consumed a canned reply: {text:?}"
    );
}

/// Attribution is decided from the header section alone: a foreign probe that frames the expected
/// method and path, or that spells the key in its body, is ordinary traffic and is rejected — so
/// the test's own request still receives the first canned reply.
#[test]
fn listener_rejects_foreign_probes_without_consuming_the_canned_reply() {
    let (base_url, server) = local_server(
        ExpectedRequest::get("/batches"),
        vec![Reply::Json(
            r#"{"data":[],"hasMore":false,"nextCursor":null}"#,
        )],
    );

    // Fragmented across three writes, unattributed, and correctly framed.
    let mut probe = Probe::connect(&base_url);
    probe.send(b"POST /batches HTTP/1.1\r\nhost: fixture\r\n");
    probe.send(b"content-length: 15\r\n\r\n");
    probe.send(b"{\"probe\":\"one\"}");
    assert_rejected(&mut probe, "a fragmented foreign request");

    // The key appears only as a body line, in a request whose method and path are exactly the
    // expected ones: a header-only reading must not attribute it.
    let marker = format!("x-api-key: {TEST_API_KEY}");
    let mut probe = Probe::connect(&base_url);
    probe.send(
        format!(
            "GET /batches HTTP/1.1\r\nhost: fixture\r\ncontent-length: {}\r\n\r\n",
            marker.len()
        )
        .as_bytes(),
    );
    probe.send(marker.as_bytes());
    assert_rejected(&mut probe, "a key-shaped body line");

    for (label, bytes) in [
        (
            "a header line without a colon",
            &b"GET /batches HTTP/1.1\r\nno-colon\r\n\r\n"[..],
        ),
        (
            "a non-numeric Content-Length",
            &b"GET /batches HTTP/1.1\r\ncontent-length: eleven\r\n\r\n"[..],
        ),
        (
            "a declared body beyond the byte cap",
            &b"GET /batches HTTP/1.1\r\ncontent-length: 70000\r\n\r\n"[..],
        ),
        (
            "Transfer-Encoding framing",
            &b"GET /batches HTTP/1.1\r\ntransfer-encoding: chunked\r\n\r\n"[..],
        ),
        (
            "invalid UTF-8 in the header section",
            &b"GET /batches HTTP/1.1\r\nx-probe: \xff\xfe\r\n\r\n"[..],
        ),
        (
            "invalid UTF-8 in the framed body",
            &b"POST /batches HTTP/1.1\r\ncontent-length: 2\r\n\r\n\xff\xfe"[..],
        ),
    ] {
        let mut probe = Probe::connect(&base_url);
        probe.send(bytes);
        assert_rejected(&mut probe, label);
    }

    // Two copies of the key with conflicting values are ambiguous ownership, not the test's request,
    // so the duplicate-key rule still refuses it when attribution reads raw header bytes.
    let mut probe = Probe::connect(&base_url);
    probe.send(
        format!(
            "GET /batches HTTP/1.1\r\nhost: fixture\r\nx-api-key: {TEST_API_KEY}\r\nx-api-key: not-the-key\r\n\r\n"
        )
        .as_bytes(),
    );
    assert_rejected(&mut probe, "two conflicting copies of the key header");

    // A foreign request whose headers plus declared body cross the byte cap is refused by the frame
    // check rather than the framing check, and still never consumes the canned reply.
    let mut probe = Probe::connect(&base_url);
    probe.send(
        format!(
            "GET /batches HTTP/1.1\r\nhost: fixture\r\ncontent-length: {MAX_REQUEST_BYTES}\r\n\r\n"
        )
        .as_bytes(),
    );
    assert_rejected(&mut probe, "an over-cap foreign request");

    let output = run_owned(&paginated_args(&base_url));
    let requests = server.join().expect("foreign probe server");
    assert_eq!(
        requests.len(),
        1,
        "{:?}",
        requests
            .iter()
            .map(TestRequest::render)
            .collect::<Vec<String>>()
    );
    assert_eq!(requests[0].method, "GET");
    assert!(
        requests[0].target.starts_with("/batches"),
        "{}",
        requests[0].target
    );
    assert!(requests[0].body.is_empty());
    let pages = ndjson(&output.stdout);
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0]["data"]["data"], serde_json::json!([]));
}

/// A request carrying the test's key but not the expected shape fails the fixture loudly, before
/// any canned reply is written. Attribution survives a header section this parse cannot read, and a
/// request whose header section plus declared body crosses the byte cap is never read as a request.
#[test]
fn listener_fails_loudly_on_attributed_requests_that_do_not_match() {
    const BODY: &str = r#"{"query":"finish the research"}"#;
    // The marker sits in another header's value instead of the framed body.
    let marker_in_header = format!(
        "POST /agent/runs HTTP/1.1\r\nhost: fixture\r\nx-api-key: {TEST_API_KEY}\r\nx-note: {BODY}\r\ncontent-length: 2\r\n\r\n{{}}"
    );
    let unframable = format!(
        "POST /agent/runs HTTP/1.1\r\nhost: fixture\r\nx-api-key: {TEST_API_KEY}\r\ncontent-length: abc\r\n\r\n"
    );
    // The key is already present when a later header value turns out not to be UTF-8.
    let mut key_then_non_utf8 = format!(
        "POST /agent/runs HTTP/1.1\r\nhost: fixture\r\nx-api-key: {TEST_API_KEY}\r\nx-probe: "
    )
    .into_bytes();
    key_then_non_utf8.extend_from_slice(b"\xff\xfe\r\ncontent-length: 2\r\n\r\n{}");
    // A malformed line before the key leaves the key outside any successfully parsed prefix.
    let malformed_before_key = format!(
        "POST /agent/runs HTTP/1.1\r\nhost: fixture\r\nno-colon\r\nx-api-key: {TEST_API_KEY}\r\ncontent-length: 2\r\n\r\n{{}}"
    )
    .into_bytes();
    // A body of exactly the cap still crosses the framed-request cap once any header section is
    // added, so it must be refused before the body is read.
    let at_cap_body_over_cap_request = format!(
        "POST /agent/runs HTTP/1.1\r\nhost: fixture\r\nx-api-key: {TEST_API_KEY}\r\ncontent-length: {MAX_REQUEST_BYTES}\r\n\r\n"
    )
    .into_bytes();
    // Each case names the failure it must reach: a fixture that failed for any other reason, or
    // that served the request instead, does not satisfy it.
    let cases: [(&str, Vec<u8>, &str); 8] = [
        (
            "wrong method",
            attributed_request("PUT", "/agent/runs", BODY),
            "did not match",
        ),
        (
            "wrong path",
            attributed_request("POST", "/agent/runs/other", BODY),
            "did not match",
        ),
        (
            "wrong body",
            attributed_request("POST", "/agent/runs", "{}"),
            "did not match",
        ),
        (
            "marker outside the body",
            marker_in_header.into_bytes(),
            "did not match",
        ),
        (
            "unframable attributed request",
            unframable.into_bytes(),
            "Content-Length was not a number",
        ),
        (
            "valid key followed by invalid UTF-8",
            key_then_non_utf8,
            "the header section was not UTF-8",
        ),
        (
            "malformed line before the valid key",
            malformed_before_key,
            "a header line carried no colon",
        ),
        (
            "headers plus an at-cap declared body",
            at_cap_body_over_cap_request,
            "the framed request exceeded the request byte cap",
        ),
    ];
    for (label, request, expected_reason) in cases {
        let (base_url, server) = local_server(
            ExpectedRequest::post("/agent/runs", "/query", "finish the research"),
            vec![Reply::Sse("data: [DONE]\n\n")],
        );
        let mut probe = Probe::connect(&base_url);
        probe.send(&request);
        let reply = probe.receive(Duration::from_secs(10));
        assert!(
            reply.is_empty(),
            "{label} received a reply instead of failing the fixture: {}",
            String::from_utf8_lossy(&reply)
        );
        let payload = match server.join() {
            Ok(_) => panic!("{label} was served as if it were the expected request"),
            Err(payload) => payload,
        };
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| payload.downcast_ref::<&str>().map(|text| text.to_string()))
            .unwrap_or_default();
        assert!(
            message.contains(expected_reason),
            "{label} failed for an unexpected reason: {message}"
        );
    }
}

/// A valid request is served once, and only once, its framed body has arrived: the declared
/// Content-Length is what ends the request, not the end of the header section.
#[test]
fn listener_serves_a_fragmented_request_only_after_its_framed_body() {
    const BODY: &str = r#"{"query":"finish the research"}"#;
    let (base_url, server) = local_server(
        ExpectedRequest::post("/agent/runs", "/query", "finish the research"),
        vec![Reply::Json(r#"{"id":"agent_run_fragmented"}"#)],
    );
    let declared = BODY.len().to_string();
    let mut probe = Probe::connect(&base_url);
    probe.send(b"POST /agent/runs HTTP/1.1\r\nhost: fixture\r\n");
    probe.send(
        format!("x-api-key: {TEST_API_KEY}\r\ncontent-length: {declared}\r\n\r\n").as_bytes(),
    );
    probe.send(&BODY.as_bytes()[..8]);

    let early = probe.receive(Duration::from_secs(1));
    assert!(
        early.is_empty(),
        "the fixture replied before the framed body arrived: {}",
        String::from_utf8_lossy(&early)
    );

    probe.send(&BODY.as_bytes()[8..]);
    let reply = probe.receive(Duration::from_secs(10));
    assert!(reply.starts_with(b"HTTP/1.1 200 OK"), "{}", probe.text());
    assert!(probe.text().contains("agent_run_fragmented"));

    let requests = server.join().expect("fragmented request server");
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].header("content-length"),
        Some(declared.as_str())
    );
    assert_eq!(requests[0].body_text(), BODY);
}

/// Slow input is abandoned within the fixture's absolute budget. Every byte resets an inactivity
/// timeout, so only a deadline shared by accept and reads can end this connection.
#[test]
fn listener_abandons_slow_input_within_its_absolute_budget() {
    let budget = Duration::from_secs(2);
    let (base_url, server) = local_server_with_budget(
        ExpectedRequest::get("/batches"),
        vec![Reply::Json(
            r#"{"data":[],"hasMore":false,"nextCursor":null}"#,
        )],
        budget,
    );
    let started = Instant::now();
    let mut probe = Probe::connect(&base_url);
    probe.send(b"GET /batches HTTP/1.1\r\nhost: fixture\r\nx-probe: ");
    let rejection = loop {
        assert!(
            started.elapsed() < budget + Duration::from_secs(8),
            "the fixture never abandoned slow input: {:?}",
            probe.text()
        );
        probe.send(b"y");
        let received = probe.receive(Duration::from_millis(100));
        if !received.is_empty() {
            break received;
        }
    };
    probe.receive(Duration::from_secs(1));
    assert!(
        String::from_utf8_lossy(&rejection).starts_with("HTTP/1.1 404"),
        "{:?}",
        probe.text()
    );
    assert!(!probe.text().contains("200 OK"), "{:?}", probe.text());
    assert!(
        server.join().is_err(),
        "the fixture kept waiting for a request that never arrived"
    );
}
