use std::borrow::Cow;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard};

use exa_agent_cli::pending::{append_pending_run, pending_runs_path, PendingRunRecord, SCHEMA};
use serde_json::Value;

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn temp_path(name: &str) -> PathBuf {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "exa-agent-pending-test-{name}-{}-{id}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_file(&dir);
    dir
}

struct EnvGuard {
    key: &'static str,
    previous: Option<String>,
    _lock: MutexGuard<'static, ()>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let lock = ENV_LOCK.lock().unwrap();
        let previous = std::env::var(key).ok();
        unsafe { std::env::set_var(key, value) };
        Self {
            key,
            previous,
            _lock: lock,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => unsafe { std::env::set_var(self.key, value) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

struct SourceDateEpochGuard {
    previous: Option<String>,
    _lock: MutexGuard<'static, ()>,
}

impl SourceDateEpochGuard {
    fn set(value: &str) -> Self {
        let lock = ENV_LOCK.lock().unwrap();
        let previous = std::env::var("SOURCE_DATE_EPOCH").ok();
        unsafe { std::env::set_var("SOURCE_DATE_EPOCH", value) };
        Self {
            previous,
            _lock: lock,
        }
    }
}

impl Drop for SourceDateEpochGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => unsafe { std::env::set_var("SOURCE_DATE_EPOCH", value) },
            None => unsafe { std::env::remove_var("SOURCE_DATE_EPOCH") },
        }
    }
}

fn record(request_id: &str) -> PendingRunRecord<'_> {
    PendingRunRecord {
        operation_id: Some("createAgentRun"),
        command: Cow::Borrowed("agent runs create"),
        api_path: "/agent/runs",
        request_id,
        idempotency_key: Some("idem-123"),
        recovery_command: "exa-agent agent runs list --compact",
    }
}

fn read_lines(path: &PathBuf) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn creates_parent_dir_and_appends_one_valid_json_line() {
    let _epoch = SourceDateEpochGuard::set("12345");
    let path = temp_path("creates-parent").join("state/pending-runs.jsonl");

    append_pending_run(&path, &record("req_one")).unwrap();

    let lines = read_lines(&path);
    assert_eq!(lines.len(), 1);
    let value: Value = serde_json::from_str(&lines[0]).unwrap();
    let keys: Vec<_> = value.as_object().unwrap().keys().cloned().collect();
    assert_eq!(
        keys,
        vec![
            "schema",
            "attemptedAt",
            "command",
            "operationId",
            "apiPath",
            "requestId",
            "idempotencyKey",
            "recoveryCommand",
        ]
    );
    assert_eq!(value["schema"], SCHEMA);
    assert_eq!(value["attemptedAt"], 12345);
    assert_eq!(value["command"], "agent runs create");
    assert_eq!(value["operationId"], "createAgentRun");
    assert_eq!(value["apiPath"], "/agent/runs");
    assert_eq!(value["requestId"], "req_one");
    assert_eq!(value["idempotencyKey"], "idem-123");
    assert_eq!(
        value["recoveryCommand"],
        "exa-agent agent runs list --compact"
    );
}

#[test]
fn appends_without_overwriting_existing_records() {
    let _epoch = SourceDateEpochGuard::set("12345");
    let path = temp_path("append").join("pending.jsonl");

    append_pending_run(&path, &record("req_one")).unwrap();
    append_pending_run(&path, &record("req_two")).unwrap();

    let lines = read_lines(&path);
    assert_eq!(lines.len(), 2);
    assert_eq!(
        serde_json::from_str::<Value>(&lines[0]).unwrap()["requestId"],
        "req_one"
    );
    assert_eq!(
        serde_json::from_str::<Value>(&lines[1]).unwrap()["requestId"],
        "req_two"
    );
}

#[test]
fn surfaces_io_errors() {
    let blocked_parent = temp_path("io-error");
    fs::write(&blocked_parent, b"not a directory").unwrap();

    let err = append_pending_run(blocked_parent.join("pending.jsonl"), &record("req_blocked"))
        .unwrap_err();

    assert!(matches!(
        err.kind(),
        std::io::ErrorKind::AlreadyExists | std::io::ErrorKind::NotADirectory
    ));
}

#[test]
fn scrubs_secret_shaped_serialized_values() {
    let _epoch = SourceDateEpochGuard::set("12345");
    let path = temp_path("redaction").join("pending.jsonl");
    let record = PendingRunRecord {
        operation_id: Some("createAgentRun"),
        command: Cow::Borrowed("agent runs create"),
        api_path: "/agent/runs?token=sk-exa-secret-1234",
        request_id: "req_redact",
        idempotency_key: Some("sk-exa-secret-5678"),
        recovery_command: "exa-agent agent runs list --limit 10 --token sk-exa-secret-9012",
    };

    append_pending_run(&path, &record).unwrap();

    let raw = fs::read_to_string(&path).unwrap();
    assert!(!raw.contains("sk-exa-secret-1234"));
    assert!(!raw.contains("sk-exa-secret-5678"));
    assert!(!raw.contains("sk-exa-secret-9012"));
    assert!(raw.contains("<redacted>"));

    let value: Value = serde_json::from_str(raw.trim()).unwrap();
    assert_eq!(value["apiPath"], "/agent/runs?token=<redacted>");
    assert_eq!(value["idempotencyKey"], "<redacted>");
    assert_eq!(
        value["recoveryCommand"],
        "exa-agent agent runs list --limit 10 --token <redacted>"
    );
}

#[test]
fn pending_runs_path_honors_explicit_override() {
    let path = temp_path("path-override").join("custom.jsonl");
    let _override = EnvGuard::set("EXA_AGENT_PENDING_RUNS", &path.display().to_string());

    assert_eq!(pending_runs_path(), path);
}
