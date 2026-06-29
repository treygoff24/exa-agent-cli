//! Parser tests for the v1 typed command tree (Wave 1A/1C skeleton).

use clap::Parser;
use exa_agent_cli::cli::{command_path, Cli, Command};
use std::process::{Command as ProcessCommand, Output};

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
    ProcessCommand::new(env!("CARGO_BIN_EXE_exa-agent"))
        .args(args)
        .env_remove("EXA_OUTPUT")
        .output()
        .unwrap_or_else(|e| panic!("failed to run exa-agent {args:?}: {e}"))
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
    assert!(!dbg.contains("header-secret"));
    assert!(!dbg.contains("service-key-secret"));
    assert!(!dbg.contains("set-secret"));
    assert!(!dbg.contains("body-secret"));
    assert!(!dbg.contains("query-secret"));
    assert!(dbg.contains("<redacted>"));
}
