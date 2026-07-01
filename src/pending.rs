//! Append-only pending-run records for ambiguous create failures.

use std::borrow::Cow;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::registry::OperationDef;

pub const SCHEMA: &str = "exa.cli.pending_run.v1";

#[cfg(test)]
static TEST_PENDING_RUNS_PATH: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

pub struct PendingRunRecord<'a> {
    pub operation_id: Option<&'a str>,
    pub command: Cow<'a, str>,
    pub api_path: &'a str,
    pub request_id: &'a str,
    pub idempotency_key: Option<&'a str>,
    pub recovery_command: &'a str,
}

impl<'a> PendingRunRecord<'a> {
    pub fn for_operation(
        op: &'a OperationDef,
        request_id: &'a str,
        idempotency_key: Option<&'a str>,
        recovery_command: &'a str,
    ) -> Self {
        Self {
            operation_id: Some(op.operation_id),
            command: Cow::Owned(op.command()),
            api_path: op.api_path,
            request_id,
            idempotency_key,
            recovery_command,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsonRecord {
    schema: &'static str,
    attempted_at: u64,
    command: String,
    operation_id: Option<String>,
    api_path: String,
    request_id: String,
    idempotency_key: Option<String>,
    recovery_command: String,
}

pub fn append_pending_run(path: impl AsRef<Path>, record: &PendingRunRecord<'_>) -> io::Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }

    let mut line = serde_json::to_vec(&to_json_record(record))
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    line.push(b'\n');

    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(&line)?;
    file.flush()
}

pub fn pending_runs_path() -> PathBuf {
    #[cfg(test)]
    {
        if let Some(path) = TEST_PENDING_RUNS_PATH.lock().unwrap().clone() {
            return path;
        }
    }
    if let Ok(path) = std::env::var("EXA_AGENT_PENDING_RUNS") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    pending_state_dir().join("pending-runs.jsonl")
}

fn pending_state_dir() -> PathBuf {
    if let Ok(path) = std::env::var("EXA_AGENT_STATE") {
        if !path.trim().is_empty() {
            return PathBuf::from(path);
        }
    }
    if let Ok(path) = std::env::var("XDG_STATE_HOME") {
        if !path.trim().is_empty() {
            return PathBuf::from(path).join("exa-agent-cli");
        }
    }
    std::env::var("HOME")
        .map(|home| {
            PathBuf::from(home)
                .join(".local")
                .join("state")
                .join("exa-agent-cli")
        })
        .unwrap_or_else(|_| PathBuf::from(".local/state/exa-agent-cli"))
}

#[cfg(test)]
pub fn set_test_pending_runs_path(path: Option<PathBuf>) {
    *TEST_PENDING_RUNS_PATH.lock().unwrap() = path;
}

fn to_json_record(record: &PendingRunRecord<'_>) -> JsonRecord {
    JsonRecord {
        schema: SCHEMA,
        attempted_at: now_epoch_seconds(),
        command: record.command.to_string(),
        operation_id: record.operation_id.map(str::to_string),
        api_path: record.api_path.to_string(),
        request_id: record.request_id.to_string(),
        idempotency_key: record.idempotency_key.map(str::to_string),
        recovery_command: record.recovery_command.to_string(),
    }
}

fn now_epoch_seconds() -> u64 {
    std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        })
}
