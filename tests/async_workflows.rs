use serde_json::Value;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

fn request_complete(bytes: &[u8]) -> bool {
    let Some(header_end) = bytes.windows(4).position(|window| window == b"\r\n\r\n") else {
        return false;
    };
    let header_end = header_end + 4;
    let headers = String::from_utf8_lossy(&bytes[..header_end]);
    let length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    bytes.len() >= header_end + length
}

fn read_request(stream: &mut std::net::TcpStream) -> String {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => bytes.extend_from_slice(&chunk[..n]),
            Err(err) if matches!(err.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) => break,
            Err(err) => panic!("read request: {err}"),
        }
        if request_complete(&bytes) {
            break;
        }
    }
    String::from_utf8(bytes).expect("request UTF-8")
}

fn local_server(replies: Vec<Reply>) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let mut requests = Vec::new();
        for reply in replies {
            let (mut stream, _) = listener.accept().expect("accept request");
            requests.push(read_request(&mut stream));
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
        "test-key-abcdef12".into(),
        "--compact".into(),
    ]
}

#[test]
fn all_ndjson_omits_intermediate_continuations_but_keeps_early_stop_recovery() {
    let (base_url, server) = local_server(vec![
        Reply::Json(r#"{"data":[{"id":"batch_1"}],"hasMore":true,"nextCursor":"cur2"}"#),
        Reply::Json(r#"{"data":[{"id":"batch_2"}],"hasMore":false,"nextCursor":null}"#),
    ]);
    let output = run_owned(&paginated_args(&base_url));
    let requests = server.join().expect("pagination server");
    assert_eq!(requests.len(), 2);
    let pages = ndjson(&output.stdout);
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0]["data"]["data"][0]["id"], "batch_1");
    assert_eq!(pages[1]["data"]["data"][0]["id"], "batch_2");
    assert_eq!(pages[0]["nextActions"], serde_json::json!([]));
    assert_eq!(pages[1]["nextActions"], serde_json::json!([]));

    let (base_url, server) = local_server(vec![Reply::Json(
        r#"{"data":[{"id":"batch_cap"}],"hasMore":true,"nextCursor":"resume"}"#,
    )]);
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

    let (base_url, server) = local_server(vec![Reply::Json(
        r#"{"data":[{"id":"batch_repeat"}],"hasMore":true,"nextCursor":"same"}"#,
    )]);
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
    let (base_url, server) = local_server(vec![
        Reply::Json(r#"{"data":[{"id":"batch_1"}],"hasMore":true,"nextCursor":"cur2"}"#),
        Reply::Json(r#"{"data":[{"id":"batch_2"}],"hasMore":false,"nextCursor":null}"#),
    ]);
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
    let (base_url, server) = local_server(vec![Reply::Drop]);
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
    let (base_url, server) = local_server(vec![Reply::Drop]);
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

    let (base_url, server) = local_server(vec![Reply::Json(
        r#"{"data":[{"id":"new-result"}],"hasMore":true,"nextCursor":"next-page"}"#,
    )]);
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
        "test-key-abcdef12".into(),
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
    let (base_url, server) = local_server(vec![
        Reply::Json(r#"{"data":[{"id":"saved"}],"hasMore":true,"nextCursor":"cur2"}"#),
        Reply::Drop,
    ]);
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
    let (base_url, server) = local_server(vec![Reply::Json(
        r#"{"data":[{"id":"never-saved"}],"hasMore":false,"nextCursor":null}"#,
    )]);
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
    let (base_url, server) = local_server(vec![Reply::Sse(complete)]);
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
        "test-key-abcdef12".into(),
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
    let (base_url, server) = local_server(vec![Reply::Sse(trailing_usage)]);
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
        "test-key-abcdef12".into(),
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
    let (base_url, server) = local_server(vec![Reply::Sse(incomplete)]);
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
        "test-key-abcdef12".into(),
        "--compact".into(),
    ]);
    server.join().expect("incomplete agent stream server");
    let terminal = stdout_json(&output);
    assert_eq!(terminal["nextActions"], serde_json::json!([]));
}

fn run_ambiguous_create(
    mut args: Vec<String>,
    extra_env: &[(&str, &str)],
) -> (Output, Vec<String>) {
    let (base_url, server) = local_server(vec![Reply::Drop]);
    args.extend([
        "--retry".into(),
        "0".into(),
        "--base-url".into(),
        base_url,
        "--api-key".into(),
        "test-key-abcdef12".into(),
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
