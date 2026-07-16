use exa_agent_cli::output::envelope::capabilities;
use exa_agent_cli::registry::{self, FieldDef, FieldKind, Namespace};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const EXPECTED_OP_COUNT: usize = 68;
const MANIFEST: &str = "tests/request_corpus/manifest.toml";

#[derive(Debug, Deserialize)]
struct Manifest {
    ops: BTreeMap<String, ManifestEntry>,
}

#[derive(Debug, Deserialize)]
struct ManifestEntry {
    argv: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    stdin: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
enum ConstraintKind {
    Enum,
    Range,
}

const EXPECTED_CONSTRAINTS: &[(&str, &str, ConstraintKind)] = &[
    ("createAgentRun", "effort", ConstraintKind::Enum),
    ("findSimilar", "category", ConstraintKind::Enum),
    ("findSimilar", "num-results", ConstraintKind::Range),
    ("search", "category", ConstraintKind::Enum),
    ("search", "num-results", ConstraintKind::Range),
    ("search", "type", ConstraintKind::Enum),
    (
        "websets-enrichments-create",
        "enrichment-format",
        ConstraintKind::Enum,
    ),
    (
        "websets-enrichments-update",
        "enrichment-format",
        ConstraintKind::Enum,
    ),
];

#[test]
fn registry_has_68_ops_and_manifest_covers_all() {
    let manifest = load_manifest();
    assert_eq!(registry::REGISTRY.len(), EXPECTED_OP_COUNT);

    let registry_ids: BTreeSet<_> = registry::REGISTRY
        .iter()
        .map(|op| op.operation_id.to_string())
        .collect();
    let manifest_ids: BTreeSet<_> = manifest.ops.keys().cloned().collect();
    let missing: Vec<_> = registry_ids.difference(&manifest_ids).cloned().collect();
    let extra: Vec<_> = manifest_ids.difference(&registry_ids).cloned().collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "manifest must cover every registry op exactly\nmissing: {missing:#?}\nextra: {extra:#?}"
    );

    for op in registry::REGISTRY {
        let entry = manifest
            .ops
            .get(op.operation_id)
            .unwrap_or_else(|| panic!("manifest missing {}", op.operation_id));
        assert!(
            !entry.argv.is_empty(),
            "manifest argv for {} must be non-empty",
            op.operation_id
        );
    }
}

#[test]
fn every_op_dry_run_emits_pipeline_preview_and_no_network() {
    let manifest = load_manifest();
    let mut exercised = 0usize;

    for op in registry::REGISTRY {
        let entry = manifest_entry(&manifest, op.operation_id);
        let closed_base_url = closed_local_base_url();
        let mut args = entry.argv.clone();
        let mut extra_env = BTreeMap::new();
        match op.namespace {
            Namespace::Api => args.extend(["--base-url".to_string(), closed_base_url]),
            Namespace::Service => {
                extra_env.insert("EXA_ADMIN_BASE_URL".to_string(), closed_base_url);
            }
        }
        args.extend([
            "--dry-run".to_string(),
            "--print-request".to_string(),
            "--compact".to_string(),
        ]);
        let output = run_args(&args, &entry.env, &extra_env, entry.stdin.as_deref());
        assert!(
            output.status.success(),
            "{} dry-run failed\nargv: {:?}\nstdout:\n{}\nstderr:\n{}",
            op.operation_id,
            args,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let value = stdout_json(&output, op.operation_id);
        assert_eq!(
            value["schema"], "exa.cli.response.v1",
            "{}",
            op.operation_id
        );
        assert_eq!(value["ok"], true, "{}", op.operation_id);
        assert_eq!(
            value
                .pointer("/operation/operationId")
                .and_then(Value::as_str),
            Some(op.operation_id),
            "{}",
            op.operation_id
        );
        assert_eq!(
            value.pointer("/data/dryRun").and_then(Value::as_bool),
            Some(true),
            "{}",
            op.operation_id
        );
        let request = value
            .pointer("/data/request")
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("{} missing data.request", op.operation_id));
        for key in ["method", "path", "body"] {
            assert!(
                request.contains_key(key),
                "{} preview request missing `{key}`",
                op.operation_id
            );
        }
        assert_eq!(
            request.get("method").and_then(Value::as_str),
            Some(op.method.as_str()),
            "{}",
            op.operation_id
        );
        assert_preview_path(op, request.get("path").and_then(Value::as_str));
        if op.fields.iter().any(|field| field.required) {
            assert!(
                request.get("body").is_some_and(Value::is_object),
                "{} preview body must be an object for required fields",
                op.operation_id
            );
        }
        assert_eq!(
            value.pointer("/request/requestId").and_then(Value::as_str),
            Some("req_dry_run"),
            "{}",
            op.operation_id
        );
        assert!(
            value
                .pointer("/request/upstreamRequestId")
                .is_none_or(Value::is_null),
            "{} should not have an upstream request id in dry-run",
            op.operation_id
        );
        exercised += 1;
    }

    assert_eq!(exercised, EXPECTED_OP_COUNT);
    eprintln!("registry_properties: dry_run_ops={exercised}");
}

#[test]
fn every_secret_capture_op_redacts_secret_from_stdout() {
    let manifest = load_manifest();
    let secret_ids: BTreeSet<_> = registry::REGISTRY
        .iter()
        .filter(|op| op.secret_capture().is_some())
        .map(|op| op.operation_id)
        .collect();
    assert_eq!(
        secret_ids,
        BTreeSet::from(["create-api-key", "createMonitor", "webhooks-create"])
    );

    let mut exercised = 0usize;
    for op in registry::REGISTRY
        .iter()
        .filter(|op| op.secret_capture().is_some())
    {
        let (response_field, output_flag, _) = op.secret_capture().unwrap();
        let raw_secret = format!("registry_property_secret_{}", op.operation_id);
        let response = serde_json::json!({
            "id": format!("{}_id", op.operation_id),
            response_field: raw_secret,
        })
        .to_string();
        let (base_url, server) = local_json_server(response);
        let secret_path = temp_path(op.operation_id).join("secret.txt");

        let entry = manifest_entry(&manifest, op.operation_id);
        let mut args = entry.argv.clone();
        args.push(output_flag.to_string());
        args.push(secret_path.to_string_lossy().into_owned());

        let mut extra_env = BTreeMap::new();
        match op.namespace {
            Namespace::Api => {
                args.extend([
                    "--base-url".to_string(),
                    base_url,
                    "--api-key".to_string(),
                    "test-key-abcdef12".to_string(),
                ]);
            }
            Namespace::Service => {
                extra_env.insert(
                    "EXA_SERVICE_KEY".to_string(),
                    "svc-admin-secret".to_string(),
                );
                extra_env.insert("EXA_ADMIN_BASE_URL".to_string(), base_url);
            }
        }
        args.push("--compact".to_string());

        let output = run_args(&args, &entry.env, &extra_env, entry.stdin.as_deref());
        let request = server.join().expect("local secret server panicked");
        assert!(
            request.starts_with(&format!("{} {} ", op.method.as_str(), op.api_path)),
            "{} sent unexpected request:\n{request}",
            op.operation_id
        );
        assert!(
            output.status.success(),
            "{} live secret-capture path failed\nstdout:\n{}\nstderr:\n{}",
            op.operation_id,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );

        let stdout = String::from_utf8(output.stdout).expect("stdout utf8");
        assert!(
            !stdout.contains(&raw_secret),
            "{} leaked raw secret to stdout: {stdout}",
            op.operation_id
        );
        let captured = fs::read_to_string(&secret_path).expect("captured secret file");
        assert_eq!(captured, raw_secret, "{}", op.operation_id);
        exercised += 1;
    }

    assert_eq!(exercised, 3);
    eprintln!("registry_properties: secret_capture_ops={exercised}");
}

#[test]
fn enum_range_ops_agree_on_verdict_between_validate_input_and_live() {
    let manifest = load_manifest();
    let actual_constraints: BTreeSet<_> = registry::REGISTRY
        .iter()
        .flat_map(|op| {
            constrained_fields(op)
                .into_iter()
                .map(move |(field, kind)| (op.operation_id, field.flag, kind))
        })
        .collect();
    let expected_constraints: BTreeSet<_> = EXPECTED_CONSTRAINTS.iter().copied().collect();
    assert_eq!(
        actual_constraints, expected_constraints,
        "enum/range constraint coverage drift"
    );

    let mut exercised = 0usize;
    for op in registry::REGISTRY {
        for (field, kind) in constrained_fields(op) {
            let (valid_value, invalid_value) = values_for(field, kind);
            let expected_validate_issue = match kind {
                ConstraintKind::Enum => "invalid_enum_value",
                ConstraintKind::Range => "invalid_value",
            };
            let expected_live_issue = match (op.operation_id, field.flag, kind) {
                // KNOWN two-validator drift (tracked follow-up): search `category` is validated on
                // the live path by a bespoke normalizer emitting `invalid_value` + a rich
                // "valid categories are ..." message, while `schema validate-input` uses the generic
                // registry enum check emitting `invalid_enum_value` + a null message. Both paths
                // correctly REJECT (the verdict parity asserted below is the load-bearing invariant);
                // only the code/message differ. Codes are pinned per-path so a deliberate
                // reconciliation of the two validators updates this line on purpose.
                ("search", "category", ConstraintKind::Enum) => "invalid_value",
                _ => expected_validate_issue,
            };

            let base_body =
                preview_body(manifest_entry(&manifest, op.operation_id), op.operation_id);

            let mut invalid_body = base_body.clone();
            set_body_value(&mut invalid_body, field.body_path, invalid_value);
            let live_invalid = run_live_with_body(
                manifest_entry(&manifest, op.operation_id),
                &invalid_body,
                false,
            );
            assert!(
                !live_invalid.status.success(),
                "{} invalid {} unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
                op.operation_id,
                field.flag,
                String::from_utf8_lossy(&live_invalid.stdout),
                String::from_utf8_lossy(&live_invalid.stderr)
            );
            let live_error = stderr_json(&live_invalid, op.operation_id);
            assert_eq!(live_error["ok"], false, "{} live", op.operation_id);
            assert_eq!(
                live_error.pointer("/error/code").and_then(Value::as_str),
                Some(expected_live_issue),
                "{} live {}",
                op.operation_id,
                field.flag
            );

            let validate_invalid = validate_input(op, &invalid_body);
            assert_eq!(
                validate_invalid["valid"], false,
                "{} validate",
                op.operation_id
            );
            assert_eq!(
                validate_invalid
                    .pointer("/details/issue")
                    .and_then(Value::as_str),
                Some(expected_validate_issue),
                "{} validate {}",
                op.operation_id,
                field.flag
            );

            let mut valid_body = base_body;
            set_body_value(&mut valid_body, field.body_path, valid_value);
            // Dry-run is enough here: registry validation runs before the live/dry-run branch.
            let live_valid = run_live_with_body(
                manifest_entry(&manifest, op.operation_id),
                &valid_body,
                true,
            );
            assert!(
                live_valid.status.success(),
                "{} valid {} rejected by live dry-run\nstdout:\n{}\nstderr:\n{}",
                op.operation_id,
                field.flag,
                String::from_utf8_lossy(&live_valid.stdout),
                String::from_utf8_lossy(&live_valid.stderr)
            );
            let live_valid_json = stdout_json(&live_valid, op.operation_id);
            assert_eq!(
                live_valid_json
                    .pointer("/data/dryRun")
                    .and_then(Value::as_bool),
                Some(true),
                "{} valid live path",
                op.operation_id
            );

            let validate_valid = validate_input(op, &valid_body);
            assert_eq!(
                validate_valid["valid"], true,
                "{} validate valid",
                op.operation_id
            );
            exercised += 1;
        }
    }

    assert_eq!(exercised, expected_constraints.len());
    eprintln!("registry_properties: enum_range_pairs={exercised}; pinned={expected_constraints:?}");
}

#[test]
fn capabilities_per_op_key_set_is_pinned() {
    let caps = capabilities();
    let commands = caps["commands"].as_array().expect("commands array");
    assert_eq!(commands.len(), EXPECTED_OP_COUNT);
    let actual_op_ids: BTreeSet<_> = commands
        .iter()
        .map(|command| {
            command["operationId"]
                .as_str()
                .expect("command operationId string")
        })
        .collect();
    let expected_op_ids: BTreeSet<_> = registry::REGISTRY
        .iter()
        .map(|op| op.operation_id)
        .collect();
    assert_eq!(
        actual_op_ids, expected_op_ids,
        "capabilities operationId set drift"
    );

    let expected = BTreeSet::from([
        "apiPath",
        "contentDefaults",
        "deprecated",
        "destructive",
        "fields",
        "idempotencySensitive",
        "method",
        "namespace",
        "operationId",
        "pagination",
        "path",
        "readOnly",
        "requiresConfirm",
        "source",
        "sourceVersion",
        "streaming",
    ]);

    for command in commands {
        let object = command.as_object().expect("capability command object");
        let actual: BTreeSet<_> = object.keys().map(String::as_str).collect();
        assert_eq!(
            actual, expected,
            "{} capabilities key drift",
            command["operationId"]
        );
    }
}

#[test]
fn websets_and_team_paths_use_live_api_prefix() {
    let websets: Vec<_> = registry::REGISTRY
        .iter()
        .filter(|op| op.cli_path.first() == Some(&"websets"))
        .collect();
    assert!(!websets.is_empty(), "expected websets registry operations");
    for op in websets {
        assert!(
            op.api_path.starts_with("/websets/v0/"),
            "{} used stale path {}",
            op.operation_id,
            op.api_path
        );
        assert!(
            !op.api_path.starts_with("/v0/"),
            "{} used stale path {}",
            op.operation_id,
            op.api_path
        );
    }

    let team = registry::REGISTRY
        .iter()
        .find(|op| op.command() == "team info")
        .expect("team info op");
    assert_eq!(team.api_path, "/websets/v0/teams/me");
}

#[test]
fn set_body_value_builds_nested_objects() {
    let mut body = serde_json::json!({ "keep": true });

    set_body_value(&mut body, "a.b.c", serde_json::json!("value"));

    assert_eq!(
        body,
        serde_json::json!({
            "keep": true,
            "a": { "b": { "c": "value" } }
        })
    );
}

#[test]
fn strip_global_body_overrides_removes_body_and_set_pairs() {
    let argv = [
        "search",
        "hello",
        "--body",
        r#"{"query":"ignored"}"#,
        "--compact",
        "--set",
        "numResults=5",
        "--dry-run",
    ]
    .map(String::from);

    assert_eq!(
        strip_global_body_overrides(&argv),
        ["search", "hello", "--compact", "--dry-run"].map(String::from)
    );
}

fn load_manifest() -> Manifest {
    toml::from_str(
        &fs::read_to_string(MANIFEST)
            .unwrap_or_else(|err| panic!("failed to read {MANIFEST}: {err}")),
    )
    .unwrap_or_else(|err| panic!("failed to parse {MANIFEST}: {err}"))
}

fn manifest_entry<'a>(manifest: &'a Manifest, operation_id: &str) -> &'a ManifestEntry {
    manifest
        .ops
        .get(operation_id)
        .unwrap_or_else(|| panic!("manifest missing {operation_id}"))
}

fn run_manifest(entry: &ManifestEntry, extra_args: &[&str]) -> Output {
    let mut args = entry.argv.clone();
    args.extend(extra_args.iter().map(|arg| (*arg).to_string()));
    run_args(&args, &entry.env, &BTreeMap::new(), entry.stdin.as_deref())
}

fn run_live_with_body(entry: &ManifestEntry, body: &Value, dry_run: bool) -> Output {
    let mut args = strip_global_body_overrides(&entry.argv);
    args.push("--body".to_string());
    args.push(body.to_string());
    if dry_run {
        args.push("--dry-run".to_string());
        args.push("--print-request".to_string());
    }
    args.push("--compact".to_string());
    run_args(&args, &entry.env, &BTreeMap::new(), entry.stdin.as_deref())
}

fn validate_input(op: &registry::OperationDef, body: &Value) -> Value {
    let args = vec![
        "schema".to_string(),
        "validate-input".to_string(),
        op.command(),
        "--body".to_string(),
        body.to_string(),
        "--compact".to_string(),
    ];
    let output = run_args(&args, &BTreeMap::new(), &BTreeMap::new(), None);
    assert!(
        output.status.success(),
        "{} validate-input failed\nstdout:\n{}\nstderr:\n{}",
        op.operation_id,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    stdout_json(&output, op.operation_id)
}

fn run_args(
    args: &[String],
    envs: &BTreeMap<String, String>,
    extra_envs: &BTreeMap<String, String>,
    stdin: Option<&str>,
) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_exa-agent"));
    cmd.args(args)
        .env("SOURCE_DATE_EPOCH", "0")
        .env_remove("EXA_OUTPUT")
        .env_remove("EXA_API_KEY")
        .env_remove("EXA_SERVICE_KEY")
        .env_remove("EXA_ADMIN_BASE_URL")
        .env(
            "EXA_AGENT_CONFIG",
            std::env::temp_dir()
                .join(format!("exa-agent-hermetic-{}", std::process::id()))
                .join("config.toml"),
        )
        .env(
            "EXA_AGENT_CREDENTIALS",
            std::env::temp_dir()
                .join(format!("exa-agent-hermetic-{}", std::process::id()))
                .join("credentials.json"),
        )
        .env_remove("EXA_PROFILE");
    for (key, value) in envs.iter().chain(extra_envs.iter()) {
        cmd.env(key, value);
    }

    if let Some(stdin) = stdin {
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|err| panic!("failed to spawn exa-agent {args:?}: {err}"));
        child
            .stdin
            .as_mut()
            .expect("stdin pipe")
            .write_all(stdin.as_bytes())
            .expect("write stdin");
        child
            .wait_with_output()
            .unwrap_or_else(|err| panic!("failed to wait for exa-agent {args:?}: {err}"))
    } else {
        cmd.output()
            .unwrap_or_else(|err| panic!("failed to run exa-agent {args:?}: {err}"))
    }
}

fn stdout_json(output: &Output, operation_id: &str) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "{operation_id} stdout was not JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn stderr_json(output: &Output, operation_id: &str) -> Value {
    serde_json::from_slice(&output.stderr).unwrap_or_else(|err| {
        panic!(
            "{operation_id} stderr was not JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn preview_body(entry: &ManifestEntry, operation_id: &str) -> Value {
    let output = run_manifest(entry, &["--dry-run", "--print-request", "--compact"]);
    assert!(
        output.status.success(),
        "{operation_id} preview failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let body = stdout_json(&output, operation_id)
        .pointer("/data/request/body")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));
    if body.is_null() {
        Value::Object(Default::default())
    } else {
        body
    }
}

fn constrained_fields(op: &registry::OperationDef) -> Vec<(&'static FieldDef, ConstraintKind)> {
    let mut fields = Vec::new();
    for field in op.fields {
        if field.range.is_some() {
            fields.push((field, ConstraintKind::Range));
        }
        if !field.enum_values.is_empty() {
            fields.push((field, ConstraintKind::Enum));
        }
    }
    fields
}

fn values_for(field: &FieldDef, kind: ConstraintKind) -> (Value, Value) {
    match kind {
        ConstraintKind::Enum => {
            let valid = field
                .enum_values
                .first()
                .unwrap_or_else(|| panic!("{} enum field has no values", field.flag));
            (
                Value::String((*valid).to_string()),
                Value::String("__registry_property_invalid_enum__".to_string()),
            )
        }
        ConstraintKind::Range => {
            let (min, max) = field.range.expect("range constraint");
            match field.kind {
                FieldKind::Int => (
                    serde_json::json!(min as i64),
                    serde_json::json!(max as i64 + 1),
                ),
                FieldKind::Num => (serde_json::json!(min), serde_json::json!(max + 1.0)),
                other => panic!("{} has unsupported range kind {other:?}", field.flag),
            }
        }
    }
}

fn set_body_value(body: &mut Value, path: &str, value: Value) {
    if !body.is_object() {
        *body = Value::Object(Default::default());
    }
    let mut current = body;
    let mut segments = path.split('.').peekable();
    while let Some(segment) = segments.next() {
        if segments.peek().is_none() {
            current
                .as_object_mut()
                .expect("object body")
                .insert(segment.to_string(), value);
            return;
        }
        current = current
            .as_object_mut()
            .expect("object body")
            .entry(segment.to_string())
            .or_insert_with(|| Value::Object(Default::default()));
        if !current.is_object() {
            *current = Value::Object(Default::default());
        }
    }
}

fn strip_global_body_overrides(argv: &[String]) -> Vec<String> {
    let mut stripped = Vec::with_capacity(argv.len());
    let mut iter = argv.iter();
    while let Some(arg) = iter.next() {
        if arg == "--body" || arg == "--set" {
            let _ = iter.next();
            continue;
        }
        stripped.push(arg.clone());
    }
    stripped
}

fn temp_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "exa-agent-registry-properties-{name}-{}-{nanos}",
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

fn assert_preview_path(op: &registry::OperationDef, actual: Option<&str>) {
    let actual = actual.unwrap_or_else(|| panic!("{} preview missing path", op.operation_id));
    if let Some((prefix, _)) = op.api_path.split_once('{') {
        assert!(
            actual.starts_with(prefix),
            "{} preview path `{actual}` must start with route prefix `{prefix}` from `{}`",
            op.operation_id,
            op.api_path
        );
    } else {
        assert_eq!(
            actual, op.api_path,
            "{} preview path drift",
            op.operation_id
        );
    }
}

fn local_json_server(response_body: String) -> (String, thread::JoinHandle<String>) {
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
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let request = String::from_utf8_lossy(&read_http_request(&mut stream)).into_owned();
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response_body.len()
        )
        .unwrap();
        stream.write_all(response_body.as_bytes()).unwrap();
        request
    });
    (format!("http://{addr}"), server)
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
            Err(err) if matches!(err.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => break,
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
