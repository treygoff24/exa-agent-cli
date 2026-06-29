//! Parser tests for the v1 typed command tree (Wave 1A/1C skeleton).

use clap::Parser;
use exa_agent_cli::cli::{command_path, Cli, Command};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Output, Stdio};

fn parses(args: &[&str]) -> Cli {
    let argv: Vec<String> = std::iter::once("exa-agent")
        .chain(args.iter().copied())
        .map(String::from)
        .collect();
    Cli::try_parse_from(argv).unwrap_or_else(|e| panic!("failed to parse {:?}: {e}", args))
}

fn parse_err(args: &[&str]) -> clap::Error {
    let argv: Vec<String> = std::iter::once("exa-agent")
        .chain(args.iter().copied())
        .map(String::from)
        .collect();
    Cli::try_parse_from(argv).unwrap_err()
}

fn assert_path(args: &[&str], expected: &str) {
    let cli = parses(args);
    assert_eq!(command_path(&cli.command), expected);
}

fn run(args: &[&str]) -> Output {
    run_with_env(args, &[])
}

fn command(args: &[&str]) -> ProcessCommand {
    let mut cmd = ProcessCommand::new(env!("CARGO_BIN_EXE_exa-agent"));
    cmd.args(args)
        .env_remove("EXA_OUTPUT")
        .env_remove("EXA_API_KEY")
        .env_remove("EXA_SERVICE_KEY")
        .env_remove("EXA_AGENT_CREDENTIALS")
        .env_remove("EXA_AGENT_CONFIG")
        .env_remove("EXA_PROFILE");
    cmd
}

fn run_with_env(args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = command(args);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to run exa-agent {args:?}: {e}"))
}

fn run_with_env_stdin(args: &[&str], envs: &[(&str, &str)], stdin: &str) -> Output {
    let mut cmd = command(args);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| panic!("failed to spawn exa-agent {args:?}: {e}"));
    child
        .stdin
        .as_mut()
        .expect("stdin pipe")
        .write_all(stdin.as_bytes())
        .expect("write stdin");
    child
        .wait_with_output()
        .unwrap_or_else(|e| panic!("failed to wait for exa-agent {args:?}: {e}"))
}

fn run_ok_json(args: &[&str]) -> serde_json::Value {
    let output = run(args);
    assert!(
        output.status.success(),
        "expected success for {args:?}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("stdout was not JSON for {args:?}: {e}"))
}

fn temp_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "exa-agent-cli-blackbox-{name}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn parse_capabilities_and_describe_alias() {
    assert_path(&["capabilities"], "capabilities");
    assert_path(&["describe"], "capabilities");
}

#[test]
fn parse_schema_commands() {
    assert_path(&["schema", "list"], "schema list");
    assert_path(&["schema", "show", "SearchRequest"], "schema show");
    parses(&[
        "schema",
        "export",
        "--api",
        "openapi",
        "--output",
        "exa-spec.yaml",
    ]);
    parses(&[
        "schema",
        "validate-input",
        "search",
        "--body",
        "@request.json",
    ]);
    parses(&["schema", "refresh", "--check"]);
}

#[test]
fn parse_robot_docs_commands() {
    assert_path(&["robot-docs", "guide"], "robot-docs guide");
    assert_path(&["robot-docs", "commands"], "robot-docs commands");
    assert_path(&["robot-docs", "errors"], "robot-docs errors");
    parses(&["robot-docs", "examples", "--task", "search"]);
    assert_path(&["robot-docs", "prompts"], "robot-docs prompts");
}

#[test]
fn parse_config_commands() {
    assert_path(&["config", "list"], "config list");
    assert_path(&["config", "get", "base-url"], "config get");
    assert_path(
        &["config", "set", "base-url", "https://api.exa.ai"],
        "config set",
    );
    assert_path(&["config", "unset", "base-url"], "config unset");
    assert_path(&["config", "path"], "config path");
    assert_path(&["config", "profiles", "list"], "config profiles list");
    assert_path(
        &["config", "profiles", "show", "default"],
        "config profiles show",
    );
    assert_path(
        &["config", "profiles", "use", "work"],
        "config profiles use",
    );
    assert_path(
        &["config", "profiles", "create", "staging"],
        "config profiles create",
    );
    assert_path(
        &["config", "profiles", "delete", "staging"],
        "config profiles delete",
    );
}

#[test]
fn parse_auth_commands() {
    assert_path(&["auth", "status"], "auth status");
    assert_path(&["auth", "login"], "auth login");
    assert_path(&["auth", "logout"], "auth logout");
    assert_eq!(
        parse_err(&["--api-key", "key", "--api-key-stdin", "auth", "status"]).kind(),
        clap::error::ErrorKind::ArgumentConflict
    );
    assert_eq!(
        parse_err(&["--api-key-stdin", "--service-key-stdin", "auth", "status"]).kind(),
        clap::error::ErrorKind::ArgumentConflict
    );
}

#[test]
fn parse_doctor() {
    assert_path(&["doctor"], "doctor");
    parses(&["doctor", "--online"]);
}

#[test]
fn parse_raw() {
    assert_path(&["raw", "POST", "/search"], "raw");
    parses(&["raw", "GET", "/v0/websets", "--body", "@req.json"]);
    parses(&[
        "raw",
        "GET",
        "/v0/websets",
        "--query",
        "status=running",
        "--query",
        "limit=10",
    ]);
}

#[test]
fn raw_dry_run_includes_query_preview() {
    let json = run_ok_json(&[
        "raw",
        "GET",
        "/v0/websets",
        "--query",
        "status=running",
        "--query",
        "limit=10",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(json["schema"], "exa.cli.request_preview.v1");
    assert_eq!(json["request"]["method"], "GET");
    assert_eq!(json["request"]["path"], "/v0/websets");
    assert_eq!(
        json["request"]["query"],
        serde_json::json!([
            { "name": "status", "value": "running" },
            { "name": "limit", "value": "10" }
        ])
    );
}

#[test]
fn raw_dry_run_redacts_secret_query_values() {
    let json = run_ok_json(&[
        "raw",
        "GET",
        "/v0/websets",
        "--query",
        "api_key=query-secret",
        "--query",
        "status=running",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(
        json["request"]["query"],
        serde_json::json!([
            { "name": "api_key", "value": "<redacted>" },
            { "name": "status", "value": "running" }
        ])
    );
}

#[test]
fn auth_status_uses_credentials_file_without_leaking_secret() {
    let dir = temp_path("auth-status");
    let credentials = dir.join("credentials.json");
    fs::write(&credentials, r#"{"api_key":"file-secret-1234"}"#).unwrap();
    let output = run_with_env(
        &["auth", "status", "--compact"],
        &[("EXA_AGENT_CREDENTIALS", credentials.to_str().unwrap())],
    );
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("file-secret-1234"));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["schema"], "exa.cli.auth_status.v1");
    assert_eq!(json["authenticated"], true);
    assert!(json["source"].as_str().unwrap().starts_with("file:"));
    assert_eq!(json["last4"], "1234");
}

#[test]
fn auth_login_and_logout_use_isolated_credentials_file() {
    let dir = temp_path("auth-login");
    let credentials = dir.join("credentials.json");
    let envs = [("EXA_AGENT_CREDENTIALS", credentials.to_str().unwrap())];
    let login = run_with_env_stdin(&["auth", "login", "--compact"], &envs, "login-secret-9999");
    assert!(login.status.success());
    let stdout = String::from_utf8_lossy(&login.stdout);
    assert!(!stdout.contains("login-secret-9999"));
    assert!(fs::read_to_string(&credentials)
        .unwrap()
        .contains("login-secret-9999"));

    let logout = run_with_env(&["auth", "logout", "--compact"], &envs);
    assert!(logout.status.success());
    let remaining = fs::read_to_string(&credentials).unwrap_or_default();
    assert!(!remaining.contains("login-secret-9999"));
}

#[test]
fn config_set_get_roundtrip_uses_config_override() {
    let dir = temp_path("config-roundtrip");
    let config = dir.join("config.toml");
    let envs = [("EXA_AGENT_CONFIG", config.to_str().unwrap())];
    let set = run_with_env(
        &[
            "config",
            "set",
            "base-url",
            "https://example.com",
            "--compact",
        ],
        &envs,
    );
    assert!(set.status.success());
    let get = run_with_env(&["config", "get", "base-url", "--compact"], &envs);
    assert!(get.status.success());
    let json: serde_json::Value = serde_json::from_slice(&get.stdout).unwrap();
    assert_eq!(json["schema"], "exa.cli.config_get.v1");
    assert_eq!(json["value"], "https://example.com");
}

#[test]
fn doctor_malformed_config_reports_finding_on_stdout_exit_one() {
    let dir = temp_path("doctor-bad-config");
    let config = dir.join("config.toml");
    fs::write(&config, "not = valid toml [[[\\n").unwrap();
    let output = run_with_env(
        &["doctor", "--compact"],
        &[("EXA_AGENT_CONFIG", config.to_str().unwrap())],
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["schema"], "exa.cli.doctor.v1");
    assert_eq!(json["status"], "findings");
    assert!(json["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| finding["id"] == "config.parse" && finding["status"] == "fail"));
}

#[test]
fn doctor_warn_findings_exit_one() {
    let dir = temp_path("doctor-warn");
    let config = dir.join("config.toml");
    let credentials = dir.join("missing-credentials.json");
    fs::write(&config, "base_url = \"https://api.exa.ai\"\n").unwrap();
    let output = run_with_env(
        &["doctor", "--check", "key.present", "--compact"],
        &[
            ("EXA_AGENT_CONFIG", config.to_str().unwrap()),
            ("EXA_AGENT_CREDENTIALS", credentials.to_str().unwrap()),
        ],
    );
    assert_eq!(output.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "findings");
    assert_eq!(json["ok"], false);
    assert!(json["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| finding["id"] == "key.present" && finding["status"] == "warn"));
}

#[test]
fn doctor_unknown_check_is_usage_error() {
    let output = run(&["doctor", "--check", "key.presnt", "--compact"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");
    assert!(stderr["error"]["details"]["valid"].is_array());
}

#[test]
fn doctor_key_present_detector_uses_credentials_file_without_leaking_secret() {
    let dir = temp_path("doctor-key-file");
    let credentials = dir.join("credentials.json");
    fs::write(&credentials, r#"{"api_key":"doctor-secret-5555"}"#).unwrap();
    let output = run_with_env(
        &["doctor", "--check", "key.present", "--compact"],
        &[("EXA_AGENT_CREDENTIALS", credentials.to_str().unwrap())],
    );
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("doctor-secret-5555"));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "healthy");
    assert!(json["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|finding| finding["id"] == "key.present" && finding["status"] == "ok"));
}

#[test]
fn config_errors_redact_secret_shaped_values() {
    let dir = temp_path("config-error-redaction");
    let config = dir.join("config.toml");
    let output = run_with_env(
        &[
            "config",
            "set",
            "base-url",
            "exa-secret-config-1234",
            "--compact",
        ],
        &[("EXA_AGENT_CONFIG", config.to_str().unwrap())],
    );
    assert_eq!(output.status.code(), Some(3));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("exa-secret-config-1234"));
    assert!(stderr.contains("<redacted>"));
}

#[test]
fn config_rejects_secret_shaped_key_env_and_malformed_base_url() {
    let dir = temp_path("config-validation");
    let config = dir.join("config.toml");
    let envs = [("EXA_AGENT_CONFIG", config.to_str().unwrap())];

    let profile = run_with_env(
        &["config", "profiles", "create", "work", "--compact"],
        &envs,
    );
    assert!(profile.status.success());

    let secret_env = run_with_env(
        &[
            "config",
            "set",
            "profiles.work.api-key-env",
            "sk-exa-secret-1234",
            "--compact",
        ],
        &envs,
    );
    assert_eq!(secret_env.status.code(), Some(3));
    let secret_stderr = String::from_utf8_lossy(&secret_env.stderr);
    assert!(!secret_stderr.contains("sk-exa-secret-1234"));
    assert!(!fs::read_to_string(&config)
        .unwrap()
        .contains("sk-exa-secret-1234"));

    let bad_url = run_with_env(
        &[
            "config",
            "set",
            "base-url",
            "https://not a url",
            "--compact",
        ],
        &envs,
    );
    assert_eq!(bad_url.status.code(), Some(3));
}

#[test]
fn doctor_redacts_secret_shaped_config_values() {
    let dir = temp_path("doctor-redaction");
    let config = dir.join("config.toml");
    fs::write(&config, "base_url = \"https://exa-secret-base-1234\"\n").unwrap();
    let output = run_with_env(
        &["doctor", "--check", "base-url", "--compact"],
        &[("EXA_AGENT_CONFIG", config.to_str().unwrap())],
    );
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("exa-secret-base-1234"));
    assert!(stdout.contains("<redacted>"));
}

#[test]
fn auth_status_rejects_api_shaped_service_key_for_admin_capability() {
    let output = run_with_env(
        &["auth", "status", "--compact"],
        &[("EXA_SERVICE_KEY", "exa-api-shaped-service-key")],
    );
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["canAdmin"], false);
    assert!(json["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|warning| warning
            .as_str()
            .unwrap_or_default()
            .contains("admin commands require a service key")));
}

#[cfg(unix)]
#[test]
fn auth_login_with_credentials_override_does_not_chmod_parent_dir() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let dir = temp_path("auth-login-perms");
    fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();
    let before = fs::metadata(&dir).unwrap().mode() & 0o777;
    let credentials = dir.join("credentials.json");
    let output = run_with_env_stdin(
        &["auth", "login", "--compact"],
        &[("EXA_AGENT_CREDENTIALS", credentials.to_str().unwrap())],
        "permission-secret-1234",
    );
    assert!(output.status.success());
    let after = fs::metadata(&dir).unwrap().mode() & 0o777;
    let file_mode = fs::metadata(&credentials).unwrap().mode() & 0o777;
    assert_eq!(before, 0o755);
    assert_eq!(after, 0o755);
    assert_eq!(file_mode, 0o600);
}

#[test]
fn search_dry_run_merges_body_and_set_with_redaction() {
    let json = run_ok_json(&[
        "search",
        "named query",
        "--body",
        r#"{"numResults":10,"token":"body-secret","contents":{"summary":{"query":"body summary"}}}"#,
        "--set",
        "contents.text=true",
        "--dry-run",
        "--compact",
    ]);
    let body = &json["request"]["body"];
    assert_eq!(body["query"], "named query");
    assert_eq!(body["numResults"], 10);
    assert_eq!(body["contents"]["summary"]["query"], "body summary");
    assert_eq!(body["contents"]["text"], true);
    assert_eq!(body["token"], "<redacted>");
}

#[test]
fn raw_dry_run_reads_body_and_set_then_redacts() {
    let json = run_ok_json(&[
        "raw",
        "POST",
        "/custom",
        "--body",
        r#"{"query":"keep","password":"body-secret"}"#,
        "--set",
        "token=set-secret",
        "--dry-run",
        "--compact",
    ]);
    let body = &json["request"]["body"];
    assert_eq!(body["query"], "keep");
    assert_eq!(body["password"], "<redacted>");
    assert_eq!(body["token"], "<redacted>");
}

#[test]
fn raw_body_stdin_empty_returns_no_input() {
    let output = run(&["raw", "POST", "/custom", "--body", "-", "--dry-run"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr)
        .unwrap_or_else(|e| panic!("stderr was not JSON: {e}"));
    assert_eq!(stderr["error"]["code"], "no_input");
    assert_eq!(stderr["error"]["category"], "no_input");
}

#[test]
fn set_overflow_path_returns_structured_error_not_panic() {
    let output = run(&[
        "search",
        "q",
        "--set",
        "18446744073709551615=x",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(
        output.status.code(),
        Some(1),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr)
        .unwrap_or_else(|e| panic!("stderr was not JSON: {e}"));
    assert_eq!(stderr["error"]["code"], "invalid_value");
}

#[test]
fn recognized_unimplemented_commands_return_structured_error() {
    let output = run(&["contents", "https://exa.ai", "--compact"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr)
        .unwrap_or_else(|e| panic!("stderr was not JSON: {e}"));
    assert_eq!(stderr["schema"], "exa.cli.error.v1");
    assert_eq!(stderr["error"]["code"], "not_implemented");
    assert!(stderr["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .contains("contents"));
}

#[test]
fn parse_search_core() {
    assert_path(&["search", "latest AI chip launches"], "search");
    parses(&[
        "search",
        "query",
        "-n",
        "5",
        "--type",
        "Fast",
        "--category",
        "research paper",
    ]);
    assert_eq!(
        parse_err(&["search", "query", "--type", "garbage"]).kind(),
        clap::error::ErrorKind::InvalidValue
    );
}

#[test]
fn parse_contents_answer_context_similar() {
    assert_path(&["contents", "https://exa.ai/docs"], "contents");
    parses(&["contents", "--ids", "id1", "id2"]);
    assert_eq!(
        parse_err(&["contents"]).kind(),
        clap::error::ErrorKind::MissingRequiredArgument
    );
    assert_eq!(
        parse_err(&["contents", "https://exa.ai", "--ids", "id1"]).kind(),
        clap::error::ErrorKind::ArgumentConflict
    );
    assert_path(&["answer", "What is Exa?"], "answer");
    assert_path(&["context", "rust async patterns"], "context");
    assert_path(&["similar", "https://exa.ai"], "similar");
}

#[test]
fn parse_team_info() {
    assert_path(&["team", "info"], "team info");
}

#[test]
fn parse_agent_runs_lifecycle() {
    assert_path(
        &["agent", "runs", "create", "find eval tools"],
        "agent runs create",
    );
    assert_path(&["agent", "runs", "list"], "agent runs list");
    assert_path(&["agent", "runs", "get", "agent_run_abc"], "agent runs get");
    parses(&["agent", "runs", "events", "agent_run_abc", "--stream"]);
    assert_path(
        &["agent", "runs", "cancel", "agent_run_abc"],
        "agent runs cancel",
    );
    assert_path(
        &["agent", "runs", "delete", "agent_run_abc"],
        "agent runs delete",
    );
}

#[test]
fn parse_research_commands() {
    assert_path(&["research", "create", "legacy query"], "research create");
    assert_path(&["research", "list"], "research list");
    assert_path(&["research", "get", "research_abc"], "research get");
}

#[test]
fn parse_monitor_commands() {
    assert_path(
        &["monitor", "create", "--name", "daily", "--query", "AI news"],
        "monitor create",
    );
    assert_path(&["monitor", "list"], "monitor list");
    assert_path(&["monitor", "get", "mon_abc"], "monitor get");
    assert_path(
        &["monitor", "update", "mon_abc", "--query", "new query"],
        "monitor update",
    );
    assert_path(&["monitor", "delete", "mon_abc"], "monitor delete");
    assert_path(&["monitor", "runs", "list", "mon_abc"], "monitor runs list");
}

#[test]
fn parse_websets_representative_nested() {
    assert_path(
        &[
            "websets",
            "create",
            "--query",
            "SF startups",
            "--count",
            "10",
        ],
        "websets create",
    );
    assert_path(&["websets", "list"], "websets list");
    assert_path(&["websets", "get", "webset_abc"], "websets get");
    assert_path(
        &["websets", "items", "list", "webset_abc"],
        "websets items list",
    );
    assert_path(
        &[
            "websets",
            "searches",
            "create",
            "webset_abc",
            "--query",
            "founders",
        ],
        "websets searches create",
    );
    assert_path(
        &["websets", "enrichments", "get", "webset_abc", "enr_abc"],
        "websets enrichments get",
    );
    assert_path(&["websets", "imports", "list"], "websets imports list");
    assert_path(&["websets", "monitors", "list"], "websets monitors list");
    assert_path(&["websets", "events", "list"], "websets events list");
    assert_path(
        &["websets", "webhooks", "attempts", "list", "wh_abc"],
        "websets webhooks attempts list",
    );
}

#[test]
fn parse_admin_keys_commands() {
    assert_path(
        &[
            "admin",
            "keys",
            "create",
            "--name",
            "ci-key",
            "--rate-limit",
            "100",
        ],
        "admin keys create",
    );
    assert_path(&["admin", "keys", "list"], "admin keys list");
    assert_path(&["admin", "keys", "get", "key_abc"], "admin keys get");
    assert_path(
        &["admin", "keys", "update", "key_abc", "--name", "renamed"],
        "admin keys update",
    );
    parses(&["admin", "keys", "delete", "key_abc", "--confirm", "key_abc"]);
    parses(&[
        "admin",
        "keys",
        "usage",
        "key_abc",
        "--start-date",
        "2026-01-01",
        "--end-date",
        "2026-06-01",
        "--group-by",
        "DAY",
    ]);
    assert_eq!(
        parse_err(&["admin", "keys", "usage", "key_abc", "--group-by", "decade"]).kind(),
        clap::error::ErrorKind::InvalidValue
    );
}

#[test]
fn parse_macros_ask_and_fetch() {
    assert_path(&["ask", "What changed in AI this week?"], "ask");
    assert_path(&["fetch", "https://exa.ai", "https://docs.exa.ai"], "fetch");
    assert_eq!(
        parse_err(&["fetch"]).kind(),
        clap::error::ErrorKind::MissingRequiredArgument
    );
}

#[test]
fn parse_preserves_global_flags_with_leaf_command() {
    let cli = parses(&[
        "--json",
        "--profile",
        "work",
        "search",
        "test query",
        "--correlation-id",
        "run-1",
        "--idempotency-key",
        "idem-1",
        "--input",
        "input.jsonl",
        "--input-format",
        "JSONL",
        "--set",
        "foo.bar=1",
        "--max-output-bytes",
        "1024",
    ]);
    assert!(cli.globals.json);
    assert_eq!(cli.globals.profile.as_deref(), Some("work"));
    assert_eq!(cli.globals.correlation_id.as_deref(), Some("run-1"));
    assert_eq!(cli.globals.idempotency_key.as_deref(), Some("idem-1"));
    assert!(matches!(cli.command, Command::Search(_)));
}

#[test]
fn debug_redacts_global_secret_values() {
    let cli = parses(&[
        "--api-key",
        "exa-secret-key",
        "--service-key",
        "service-secret-key",
        "--header",
        "Authorization: Bearer header-secret",
        "--header",
        "x-exa-service-key: service-key-secret",
        "--set",
        "webhookSecret=set-secret",
        "--body",
        "{\"token\":\"body-secret\"}",
        "raw",
        "GET",
        "/search",
        "--query",
        "token=query-secret",
    ]);
    let dbg = format!("{cli:?}");
    assert!(!dbg.contains("exa-secret-key"));
    assert!(!dbg.contains("service-secret-key"));
    assert!(!dbg.contains("header-secret"));
    assert!(!dbg.contains("service-key-secret"));
    assert!(!dbg.contains("set-secret"));
    assert!(!dbg.contains("body-secret"));
    assert!(!dbg.contains("query-secret"));
    assert!(dbg.contains("<redacted>"));
}
