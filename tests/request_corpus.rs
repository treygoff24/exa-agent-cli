use exa_agent_cli::registry;
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const MANIFEST: &str = "tests/request_corpus/manifest.toml";
const ALLOWED_DIFFS: &str = "tests/allowed_golden_diffs.toml";

#[derive(Debug, Deserialize)]
struct Manifest {
    ops: BTreeMap<String, CorpusEntry>,
}

#[derive(Debug, Deserialize)]
struct CorpusEntry {
    argv: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    stdin: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct AllowedDiffs {
    #[serde(default)]
    diffs: Vec<AllowedDiff>,
}

#[derive(Debug, Deserialize)]
struct AllowedDiff {
    #[serde(rename = "operationId")]
    operation_id: String,
    json_pointer: String,
    phase: String,
    reason: String,
    #[serde(default)]
    before: Option<Value>,
    #[serde(default)]
    after: Option<Value>,
}

#[derive(Debug)]
struct JsonDiff {
    pointer: String,
    before: Option<Value>,
    after: Option<Value>,
}

#[test]
fn corpus_covers_every_registry_op() {
    let manifest = load_manifest();
    let registry_ids: BTreeSet<_> = registry::REGISTRY
        .iter()
        .map(|op| op.operation_id.to_string())
        .collect();
    let manifest_ids: BTreeSet<_> = manifest.ops.keys().cloned().collect();

    let missing: Vec<_> = registry_ids.difference(&manifest_ids).cloned().collect();
    let extra: Vec<_> = manifest_ids.difference(&registry_ids).cloned().collect();

    assert!(
        missing.is_empty() && extra.is_empty(),
        "request corpus manifest must match registry\nmissing: {missing:#?}\nextra: {extra:#?}"
    );
}

#[test]
fn corpus_matches_goldens() {
    let manifest = load_manifest();
    let allowed = load_allowed_diffs();

    // Aggregate failures across all 68 ops so a wave that breaks several surfaces them in one
    // run, rather than one iterate-fix-rerun cycle per op.
    let mut failures: Vec<String> = Vec::new();
    for op in registry::REGISTRY {
        let entry = manifest
            .ops
            .get(op.operation_id)
            .unwrap_or_else(|| panic!("manifest missing {}", op.operation_id));
        let output = run_preview(entry);
        if !output.status.success() {
            failures.push(format!(
                "{}: expected preview success\nargv: {:?}\nstdout:\n{}\nstderr:\n{}",
                op.operation_id,
                entry.argv,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
            continue;
        }

        let actual = normalize_stdout(&output.stdout, op.operation_id);
        assert_request_preview(op.operation_id, &actual);

        let golden_path = golden_path(op.operation_id);
        let expected = normalize_text(
            &fs::read_to_string(&golden_path)
                .unwrap_or_else(|err| panic!("failed to read {}: {err}", golden_path.display())),
            op.operation_id,
        );

        if pretty(&expected) == pretty(&actual) {
            continue;
        }

        write_actual(op.operation_id, &pretty(&actual));
        let diffs = diff_json(&expected, &actual);
        let unexpected: Vec<_> = diffs
            .iter()
            .filter(|diff| !allowed.accepts(op.operation_id, diff))
            .collect();

        if !unexpected.is_empty() {
            failures.push(format!(
                "{}: golden mismatch\n{}",
                op.operation_id,
                format_diffs(&unexpected)
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} op(s) failed the request corpus (actual written under target/request_corpus_actual/):\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}

fn load_manifest() -> Manifest {
    toml::from_str(
        &fs::read_to_string(MANIFEST)
            .unwrap_or_else(|err| panic!("failed to read {MANIFEST}: {err}")),
    )
    .unwrap_or_else(|err| panic!("failed to parse {MANIFEST}: {err}"))
}

fn load_allowed_diffs() -> AllowedDiffs {
    let allowed: AllowedDiffs = toml::from_str(
        &fs::read_to_string(ALLOWED_DIFFS)
            .unwrap_or_else(|err| panic!("failed to read {ALLOWED_DIFFS}: {err}")),
    )
    .unwrap_or_else(|err| panic!("failed to parse {ALLOWED_DIFFS}: {err}"));
    for diff in &allowed.diffs {
        assert!(
            !(diff.operation_id.is_empty()
                || diff.json_pointer.is_empty()
                || diff.phase.is_empty()
                || diff.reason.is_empty()),
            "allowed golden diff entries must include operationId, json_pointer, phase, and reason"
        );
        assert!(
            diff.after.is_some(),
            "allowed golden diff for {} at {} must pin `after` to the exact new value (a value-blind allowance is forbidden)",
            diff.operation_id,
            diff.json_pointer
        );
    }
    allowed
}

fn run_preview(entry: &CorpusEntry) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_exa-agent"));
    cmd.args(&entry.argv)
        .args(["--dry-run", "--print-request"])
        .env("SOURCE_DATE_EPOCH", "0")
        .env_remove("EXA_OUTPUT")
        .env_remove("EXA_API_KEY")
        .env_remove("EXA_SERVICE_KEY")
        .env_remove("EXA_ADMIN_BASE_URL")
        .env_remove("EXA_AGENT_CREDENTIALS")
        .env_remove("EXA_AGENT_CONFIG")
        .env_remove("EXA_PROFILE");
    for (key, value) in &entry.env {
        cmd.env(key, value);
    }

    if let Some(stdin) = &entry.stdin {
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|err| panic!("failed to spawn exa-agent {:?}: {err}", entry.argv));
        child
            .stdin
            .as_mut()
            .expect("stdin pipe")
            .write_all(stdin.as_bytes())
            .expect("write stdin");
        child
            .wait_with_output()
            .unwrap_or_else(|err| panic!("failed to wait for exa-agent {:?}: {err}", entry.argv))
    } else {
        cmd.output()
            .unwrap_or_else(|err| panic!("failed to run exa-agent {:?}: {err}", entry.argv))
    }
}

fn normalize_stdout(stdout: &[u8], operation_id: &str) -> Value {
    normalize_text(
        std::str::from_utf8(stdout)
            .unwrap_or_else(|err| panic!("{operation_id} stdout was not UTF-8: {err}")),
        operation_id,
    )
}

fn normalize_text(text: &str, operation_id: &str) -> Value {
    let mut value: Value = serde_json::from_str(text)
        .unwrap_or_else(|err| panic!("{operation_id} output was not JSON: {err}\n{text}"));
    scrub_nondeterminism(&mut value);
    value
}

fn scrub_nondeterminism(value: &mut Value) {
    if value.get("schema").and_then(Value::as_str) == Some("exa.cli.response.v1") {
        if let Some(request_id) = value.pointer_mut("/request/requestId") {
            *request_id = Value::String("req_dry_run".to_string());
        }
        if let Some(duration) = value.pointer_mut("/diagnostics/durationMs") {
            *duration = Value::from(0);
        }
    }
    if value.get("schema").and_then(Value::as_str) == Some("exa.cli.request_preview.v1") {
        if let Some(request_id) = value.get_mut("requestId") {
            *request_id = Value::String("req_dry_run".to_string());
        }
    }
}

fn assert_request_preview(operation_id: &str, value: &Value) {
    match value.get("schema").and_then(Value::as_str) {
        Some("exa.cli.request_preview.v1") => {
            assert_eq!(value.get("ok").and_then(Value::as_bool), Some(true));
            assert_eq!(
                value
                    .pointer("/operation/operationId")
                    .and_then(Value::as_str),
                Some(operation_id)
            );
            assert_eq!(value.get("dryRun").and_then(Value::as_bool), Some(true));
            assert!(
                value.get("request").is_some(),
                "{operation_id} missing request"
            );
        }
        Some("exa.cli.response.v1") => {
            assert_eq!(value.get("ok").and_then(Value::as_bool), Some(true));
            assert_eq!(
                value
                    .pointer("/operation/operationId")
                    .and_then(Value::as_str),
                Some(operation_id)
            );
            assert_eq!(
                value.pointer("/data/dryRun").and_then(Value::as_bool),
                Some(true)
            );
            let request = value
                .pointer("/data/request")
                .and_then(Value::as_object)
                .unwrap_or_else(|| panic!("{operation_id} missing data.request preview"));
            for key in ["method", "path", "query", "body"] {
                assert!(
                    request.contains_key(key),
                    "{operation_id} preview request missing `{key}`"
                );
            }
        }
        other => panic!("{operation_id} emitted non-preview schema: {other:?}"),
    }
}

fn golden_path(operation_id: &str) -> PathBuf {
    Path::new("tests/request_corpus").join(format!("{operation_id}.json"))
}

fn pretty(value: &Value) -> String {
    format!(
        "{}\n",
        serde_json::to_string_pretty(value).expect("JSON pretty-print")
    )
}

fn write_actual(operation_id: &str, actual_pretty: &str) {
    let dir = Path::new("target/request_corpus_actual");
    fs::create_dir_all(dir).expect("create actual output dir");
    fs::write(
        dir.join(format!("{operation_id}.actual.json")),
        actual_pretty,
    )
    .expect("write actual golden mismatch output");
}

fn diff_json(expected: &Value, actual: &Value) -> Vec<JsonDiff> {
    let mut diffs = Vec::new();
    diff_value("", Some(expected), Some(actual), &mut diffs);
    diffs
}

fn diff_value(
    path: &str,
    expected: Option<&Value>,
    actual: Option<&Value>,
    out: &mut Vec<JsonDiff>,
) {
    match (expected, actual) {
        (Some(Value::Object(expected)), Some(Value::Object(actual))) => {
            let keys: BTreeSet<_> = expected.keys().chain(actual.keys()).collect();
            for key in keys {
                diff_value(
                    &join_pointer(path, key),
                    expected.get(key),
                    actual.get(key),
                    out,
                );
            }
        }
        (Some(Value::Array(expected)), Some(Value::Array(actual))) => {
            for idx in 0..expected.len().max(actual.len()) {
                diff_value(
                    &join_pointer(path, &idx.to_string()),
                    expected.get(idx),
                    actual.get(idx),
                    out,
                );
            }
        }
        (Some(expected), Some(actual)) if expected == actual => {}
        _ => out.push(JsonDiff {
            pointer: if path.is_empty() {
                "/".to_string()
            } else {
                path.to_string()
            },
            before: expected.cloned(),
            after: actual.cloned(),
        }),
    }
}

fn join_pointer(parent: &str, segment: &str) -> String {
    let escaped = segment.replace('~', "~0").replace('/', "~1");
    if parent.is_empty() {
        format!("/{escaped}")
    } else {
        format!("{parent}/{escaped}")
    }
}

impl AllowedDiffs {
    fn accepts(&self, operation_id: &str, diff: &JsonDiff) -> bool {
        self.diffs.iter().any(|allowed| {
            allowed.operation_id == operation_id
                && allowed.json_pointer == diff.pointer
                // `after` is mandatory (enforced in load_allowed_diffs) and must equal the exact
                // new value, so an allowance auto-expires the moment the value changes again — a
                // value-blind wildcard that licenses unrelated future regressions is impossible.
                && allowed.after.as_ref() == diff.after.as_ref()
                // `before` is optional, but when given it must equal the exact prior value.
                && allowed
                    .before
                    .as_ref()
                    .is_none_or(|before| diff.before.as_ref() == Some(before))
        })
    }
}

fn format_diffs(diffs: &[&JsonDiff]) -> String {
    diffs
        .iter()
        .map(|diff| {
            format!(
                "{}\n  before: {}\n  after:  {}",
                diff.pointer,
                diff.before
                    .as_ref()
                    .map(Value::to_string)
                    .unwrap_or_else(|| "<missing>".to_string()),
                diff.after
                    .as_ref()
                    .map(Value::to_string)
                    .unwrap_or_else(|| "<missing>".to_string())
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}
