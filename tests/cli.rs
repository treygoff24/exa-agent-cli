//! Parser tests for the v1 typed command tree (Wave 1A/1C skeleton).

use clap::Parser;
use exa_agent_cli::cli::{command_path, Cli, Command};
use exa_agent_cli::transport;
use std::fs;
use std::io::{ErrorKind, Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Command as ProcessCommand, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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

fn run_owned(args: &[String]) -> Output {
    let mut cmd = command(&[]);
    cmd.args(args);
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to run exa-agent {args:?}: {e}"))
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

fn local_json_server<F>(
    validate: F,
    response_body: &'static [u8],
) -> (String, thread::JoinHandle<()>)
where
    F: FnOnce(String) + Send + 'static,
{
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
        validate(String::from_utf8_lossy(&read_http_request(&mut stream)).into_owned());
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            response_body.len()
        )
        .unwrap();
        stream.write_all(response_body).unwrap();
    });
    (format!("http://{addr}"), server)
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
fn schema_commands_work_offline() {
    let show = run_ok_json(&["schema", "show", "search", "--compact"]);
    assert_eq!(show["schema"], "exa.cli.schema_show.v1");
    assert_eq!(show["ok"], true);
    assert_eq!(show["operation"]["command"], "search");

    let export = run_ok_json(&["schema", "export", "--api", "openapi", "--compact"]);
    assert_eq!(export["schema"], "exa.cli.schema_export.v1");
    assert_eq!(export["target"], "openapi");
    assert!(!export["operations"].as_array().unwrap().is_empty());

    let validate = run_ok_json(&["schema", "validate-input", "search", "--compact"]);
    assert_eq!(validate["schema"], "exa.cli.schema_validate_input.v1");
    assert_eq!(validate["valid"], true);

    let refresh = run_ok_json(&["schema", "refresh", "--check", "--compact"]);
    assert_eq!(refresh["schema"], "exa.cli.schema_refresh.v1");
    assert_eq!(refresh["status"], "current");
}

#[test]
fn robot_docs_commands_work_offline() {
    for (args, section) in [
        (vec!["robot-docs", "guide", "--compact"], "guide"),
        (vec!["robot-docs", "commands", "--compact"], "commands"),
        (vec!["robot-docs", "errors", "--compact"], "errors"),
        (
            vec!["robot-docs", "examples", "--task", "search", "--compact"],
            "examples",
        ),
        (vec!["robot-docs", "prompts", "--compact"], "prompts"),
    ] {
        let json = run_ok_json(&args);
        assert_eq!(json["schema"], "exa.cli.robot_docs.v1");
        assert_eq!(json["ok"], true);
        assert_eq!(json["section"], section);
    }
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
    assert_eq!(json["schema"], "exa.cli.response.v1");
    assert_eq!(json["ok"], true);
    assert_eq!(json["command"], "raw");
    assert!(json["dataHash"].as_str().unwrap().starts_with("sha256:"));
    assert_eq!(json["data"]["request"]["method"], "GET");
    assert_eq!(json["data"]["request"]["path"], "/v0/websets");
    assert_eq!(
        json["data"]["request"]["query"],
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
        json["data"]["request"]["query"],
        serde_json::json!([
            { "name": "api_key", "value": "<redacted>" },
            { "name": "status", "value": "running" }
        ])
    );

    let json = run_ok_json(&[
        "raw",
        "GET",
        "/v0/websets",
        "--query",
        "q=11111111-2222-3333-4444-555555555555",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(
        json["data"]["request"]["query"],
        serde_json::json!([{ "name": "q", "value": "<redacted>" }])
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
    assert_eq!(json["schema"], "exa.cli.response.v1");
    assert_eq!(json["command"], "search");
    assert!(json["dataHash"].as_str().unwrap().starts_with("sha256:"));
    let body = &json["data"]["request"]["body"];
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
    let body = &json["data"]["request"]["body"];
    assert_eq!(body["query"], "keep");
    assert_eq!(body["password"], "<redacted>");
    assert_eq!(body["token"], "<redacted>");
}

#[test]
fn raw_refuses_user_authorization_header_before_auth() {
    let output = run(&[
        "--header",
        "Authorization: Bearer user-supplied-secret",
        "raw",
        "GET",
        "/search",
        "--compact",
    ]);
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr)
        .unwrap_or_else(|e| panic!("stderr was not JSON: {e}"));
    assert_eq!(stderr["schema"], "exa.cli.error.v1");
    assert_eq!(stderr["error"]["code"], "invalid_flag_combination");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!all.contains("user-supplied-secret"));
}

#[test]
fn raw_dry_run_refuses_user_authorization_header() {
    let output = run(&[
        "--header",
        "Authorization: Bearer user-supplied-secret",
        "raw",
        "GET",
        "/search",
        "--dry-run",
        "--compact",
    ]);
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr)
        .unwrap_or_else(|e| panic!("stderr was not JSON: {e}"));
    assert_eq!(stderr["error"]["code"], "invalid_flag_combination");
    assert_eq!(stderr["operation"]["method"], "GET");
    assert_eq!(stderr["operation"]["path"], "/search");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!all.contains("user-supplied-secret"));
}

#[test]
fn raw_live_without_credential_is_not_authenticated() {
    let dir = temp_path("raw-no-credential");
    let missing_credentials = dir.join("missing-credentials.json");
    let output = run_with_env(
        &[
            "--correlation-id",
            "corr-raw-no-credential",
            "raw",
            "GET",
            "/search",
            "--compact",
        ],
        &[(
            "EXA_AGENT_CREDENTIALS",
            missing_credentials.to_str().unwrap(),
        )],
    );
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr)
        .unwrap_or_else(|e| panic!("stderr was not JSON: {e}"));
    assert_eq!(stderr["schema"], "exa.cli.error.v1");
    assert_eq!(stderr["error"]["code"], "not_authenticated");
    assert_eq!(stderr["operation"]["method"], "GET");
    assert_eq!(stderr["operation"]["path"], "/search");
    assert_eq!(stderr["request"]["correlationId"], "corr-raw-no-credential");
    assert!(stderr["request"]["requestId"]
        .as_str()
        .unwrap()
        .starts_with("req_local_"));
    assert!(stderr["error"]["details"]["checked"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value.as_str().unwrap_or("").contains("credentials")));
}

#[test]
fn raw_error_context_redacts_secret_shaped_path() {
    let dir = temp_path("raw-redact-path");
    let missing_credentials = dir.join("missing-credentials.json");
    let output = run_with_env(
        &[
            "--correlation-id",
            "11111111-2222-3333-4444-555555555555",
            "raw",
            "GET",
            "/search/11111111-2222-3333-4444-555555555555",
            "--compact",
        ],
        &[(
            "EXA_AGENT_CREDENTIALS",
            missing_credentials.to_str().unwrap(),
        )],
    );
    assert!(!output.status.success());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr)
        .unwrap_or_else(|e| panic!("stderr was not JSON: {e}"));
    assert_eq!(stderr["operation"]["path"], "/search/<redacted>");
    assert_eq!(stderr["request"]["correlationId"], "<redacted>");
    let all = String::from_utf8_lossy(&output.stderr);
    assert!(!all.contains("11111111-2222-3333-4444-555555555555"));
}

#[test]
fn raw_malformed_inputs_do_not_echo_secret_values() {
    let output = run(&[
        "--header",
        "X-Trace 11111111-2222-3333-4444-555555555555",
        "raw",
        "GET",
        "/search",
        "--dry-run",
        "--compact",
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("`--header` must be `Name: value`"));
    assert!(!stderr.contains("11111111-2222-3333-4444-555555555555"));

    let output = run(&[
        "raw",
        "GET",
        "/search",
        "--query",
        "q 11111111-2222-3333-4444-555555555555",
        "--dry-run",
        "--compact",
    ]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("raw --query expects `key=value`"));
    assert!(!stderr.contains("11111111-2222-3333-4444-555555555555"));
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
fn search_live_without_credential_is_not_authenticated() {
    let dir = temp_path("search-no-credential");
    let missing_credentials = dir.join("missing-credentials.json");
    let output = run_with_env(
        &["search", "agents", "--compact"],
        &[(
            "EXA_AGENT_CREDENTIALS",
            missing_credentials.to_str().unwrap(),
        )],
    );
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "not_authenticated");
    assert_eq!(stderr["operation"]["method"], "POST");
    assert_eq!(stderr["operation"]["path"], "/search");
}

#[test]
fn chunked_contents_live_without_credential_keeps_operation_context() {
    let dir = temp_path("contents-chunked-no-credential");
    let missing_credentials = dir.join("missing-credentials.json");
    let output = run_with_env(
        &[
            "contents",
            "https://a.test",
            "https://b.test",
            "--chunk-size",
            "1",
            "--compact",
        ],
        &[(
            "EXA_AGENT_CREDENTIALS",
            missing_credentials.to_str().unwrap(),
        )],
    );
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "not_authenticated");
    assert_eq!(stderr["operation"]["method"], "POST");
    assert_eq!(stderr["operation"]["path"], "/contents");
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
fn contents_dry_run_builds_urls_and_ids_bodies() {
    let urls = run_ok_json(&[
        "contents",
        "https://exa.ai",
        "https://docs.exa.ai",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(urls["schema"], "exa.cli.response.v1");
    assert_eq!(urls["command"], "contents");
    assert_eq!(urls["data"]["request"]["method"], "POST");
    assert_eq!(urls["data"]["request"]["path"], "/contents");
    assert_eq!(
        urls["data"]["request"]["body"]["urls"],
        serde_json::json!(["https://exa.ai", "https://docs.exa.ai"])
    );

    let ids = run_ok_json(&[
        "contents",
        "--ids",
        "id-one",
        "id-two",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(ids["command"], "contents");
    assert_eq!(
        ids["data"]["request"]["body"]["ids"],
        serde_json::json!(["id-one", "id-two"])
    );
}

#[test]
fn contents_chunk_size_dry_run_emits_one_envelope_per_chunk() {
    let output = run(&[
        "contents",
        "https://a.test",
        "https://b.test",
        "--chunk-size",
        "1",
        "--dry-run",
        "--compact",
    ]);
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<_> = stdout.lines().collect();
    assert_eq!(lines.len(), 2, "stdout:\n{stdout}");
    let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    let second: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(
        first["data"]["request"]["body"]["urls"],
        serde_json::json!(["https://a.test"])
    );
    assert_eq!(
        second["data"]["request"]["body"]["urls"],
        serde_json::json!(["https://b.test"])
    );
}

#[test]
fn contents_rejects_more_than_one_hundred_inputs_without_chunk_size() {
    let mut args = vec!["contents".to_string()];
    for n in 0..101 {
        args.push(format!("https://example.test/{n}"));
    }
    args.push("--dry-run".to_string());
    args.push("--compact".to_string());

    let output = run_owned(&args);
    assert_eq!(
        output.status.code(),
        Some(1),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_flag_combination");
    assert_eq!(stderr["operation"]["method"], "POST");
    assert_eq!(stderr["operation"]["path"], "/contents");
    assert!(stderr["error"]["suggestedCommand"]
        .as_str()
        .unwrap_or_default()
        .contains("--chunk-size 100"));
}

#[test]
fn answer_dry_run_builds_request_body_with_schema_file() {
    let dir = temp_path("answer-schema");
    let schema_path = dir.join("answer.schema.json");
    fs::write(
        &schema_path,
        r#"{"type":"object","properties":{"answer":{"type":"string"}}}"#,
    )
    .unwrap();
    let schema_arg = format!("@{}", schema_path.display());

    let output = run_owned(&[
        "answer".into(),
        "What is Exa?".into(),
        "--text".into(),
        "--stream".into(),
        "--output-schema".into(),
        schema_arg,
        "--body".into(),
        r#"{"text":false}"#.into(),
        "--set".into(),
        "stream=false".into(),
        "--dry-run".into(),
        "--compact".into(),
    ]);
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let body = &json["data"]["request"]["body"];
    assert_eq!(json["command"], "answer");
    assert_eq!(json["data"]["request"]["path"], "/answer");
    assert_eq!(body["query"], "What is Exa?");
    assert_eq!(body["text"], false);
    assert_eq!(body["stream"], false);
    assert_eq!(
        body["outputSchema"],
        serde_json::json!({"type":"object","properties":{"answer":{"type":"string"}}})
    );
}

#[test]
fn context_dry_run_omits_dynamic_tokens_and_validates_range() {
    let dynamic = run_ok_json(&[
        "context",
        "rust async patterns",
        "--tokens",
        "dynamic",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(dynamic["command"], "context");
    assert_eq!(dynamic["data"]["request"]["path"], "/context");
    assert_eq!(
        dynamic["data"]["request"]["body"],
        serde_json::json!({"query":"rust async patterns"})
    );

    let fixed = run_ok_json(&[
        "context",
        "rust async patterns",
        "--tokens",
        "1000",
        "--body",
        r#"{"tokensNum":2000}"#,
        "--set",
        "tokensNum=3000",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(fixed["data"]["request"]["body"]["tokensNum"], 3000);

    for bad in ["49", "100001", "lots"] {
        let output = run(&["context", "q", "--tokens", bad, "--dry-run", "--compact"]);
        assert_eq!(
            output.status.code(),
            Some(1),
            "bad={bad}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(stderr["error"]["code"], "invalid_value");
        assert_eq!(stderr["operation"]["path"], "/context");
    }
}

#[test]
fn context_rejects_queries_over_two_thousand_chars() {
    let ok_query = "x".repeat(2_000);
    let ok = run_owned(&[
        "context".into(),
        ok_query,
        "--dry-run".into(),
        "--compact".into(),
    ]);
    assert!(ok.status.success());

    let too_long = "x".repeat(2_001);
    let output = run_owned(&[
        "context".into(),
        too_long,
        "--dry-run".into(),
        "--compact".into(),
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");
    assert!(stderr["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .contains("2000"));
    assert_eq!(stderr["operation"]["path"], "/context");

    let body_override = run_owned(&[
        "context".into(),
        "short".into(),
        "--body".into(),
        serde_json::json!({ "query": "x".repeat(2_001) }).to_string(),
        "--dry-run".into(),
        "--compact".into(),
    ]);
    assert_eq!(body_override.status.code(), Some(1));
    assert!(body_override.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&body_override.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");
    assert_eq!(stderr["operation"]["path"], "/context");

    let set_override = run_owned(&[
        "context".into(),
        "short".into(),
        "--set".into(),
        format!("query={}", "x".repeat(2_001)),
        "--dry-run".into(),
        "--compact".into(),
    ]);
    assert_eq!(set_override.status.code(), Some(1));
    assert!(set_override.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&set_override.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_value");
    assert_eq!(stderr["operation"]["path"], "/context");
}

#[test]
fn similar_dry_run_builds_request_body() {
    let json = run_ok_json(&[
        "similar",
        "https://exa.ai",
        "--num-results",
        "7",
        "--exclude-source-domain",
        "--category",
        "company",
        "--body",
        r#"{"numResults":8}"#,
        "--set",
        "category=people",
        "--dry-run",
        "--compact",
    ]);
    let body = &json["data"]["request"]["body"];
    assert_eq!(json["command"], "similar");
    assert_eq!(json["data"]["request"]["path"], "/findSimilar");
    assert_eq!(body["url"], "https://exa.ai");
    assert_eq!(body["numResults"], 8);
    assert_eq!(body["excludeSourceDomain"], true);
    assert_eq!(body["category"], "people");
    assert_eq!(json["warnings"][0]["code"], "deprecated_upstream");
}

#[test]
fn team_info_dry_run_builds_get_path() {
    let json = run_ok_json(&["team", "info", "--dry-run", "--print-request", "--compact"]);
    assert_eq!(json["command"], "team info");
    assert_eq!(json["data"]["request"]["method"], "GET");
    assert_eq!(json["data"]["request"]["path"], "/v0/teams/me");
    assert_eq!(json["data"]["request"]["query"], serde_json::json!([]));
    assert_eq!(json["data"]["request"]["body"], serde_json::json!(null));
    assert_eq!(json["data"]["dryRun"], true);
}

#[test]
fn research_dry_run_builds_create_list_and_get_requests() {
    let create = run_ok_json(&[
        "research",
        "create",
        "legacy topic",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(create["command"], "research create");
    assert_eq!(create["data"]["request"]["path"], "/research/v1");
    assert_eq!(
        create["data"]["request"]["body"]["instructions"],
        "legacy topic"
    );
    assert_eq!(create["warnings"][0]["code"], "legacy_api");

    let list = run_ok_json(&[
        "research",
        "list",
        "--limit",
        "10",
        "--cursor",
        "cur_abc",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(list["command"], "research list");
    assert_eq!(list["data"]["request"]["path"], "/research/v1");
    assert_eq!(
        list["data"]["request"]["query"],
        serde_json::json!([
            {"name": "limit", "value": "10"},
            {"name": "cursor", "value": "cur_abc"}
        ])
    );
    assert_eq!(list["data"]["request"]["body"], serde_json::json!(null));
    assert_eq!(list["warnings"][0]["code"], "legacy_api");

    let get = run_ok_json(&["research", "get", "research/abc", "--dry-run", "--compact"]);
    assert_eq!(get["command"], "research get");
    assert_eq!(
        get["data"]["request"]["path"],
        "/research/v1/research%2Fabc"
    );
    assert_eq!(get["data"]["request"]["body"], serde_json::json!(null));
    assert_eq!(get["warnings"][0]["code"], "legacy_api");
}

#[test]
fn research_accepts_all_but_rejects_orphaned_pagination_flags_and_create_stream() {
    let list_all = run_ok_json(&["research", "list", "--all", "--dry-run", "--compact"]);
    assert_eq!(list_all["command"], "research list");
    assert_eq!(list_all["data"]["request"]["path"], "/research/v1");
    assert_eq!(list_all["data"]["request"]["query"], serde_json::json!([]));

    let max_pages = run(&[
        "research",
        "list",
        "--max-pages",
        "3",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(max_pages.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&max_pages.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_flag_combination");
    assert!(stderr["error"]["suggestedCommand"]
        .as_str()
        .unwrap()
        .contains("--all"));

    let page_delay = run(&[
        "research",
        "list",
        "--page-delay",
        "100ms",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(page_delay.status.code(), Some(1));
    let stderr: serde_json::Value = serde_json::from_slice(&page_delay.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_flag_combination");

    let create_stream = run(&[
        "research",
        "create",
        "legacy topic",
        "--stream",
        "--dry-run",
        "--compact",
    ]);
    assert_eq!(create_stream.status.code(), Some(1));
    assert!(create_stream.stdout.is_empty());
    let stderr: serde_json::Value = serde_json::from_slice(&create_stream.stderr).unwrap();
    assert_eq!(stderr["error"]["code"], "invalid_flag_combination");
    assert_eq!(stderr["operation"]["path"], "/research/v1");
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
    parses(&[
        "contents",
        "https://exa.ai/docs",
        "--text",
        "--summary-query",
        "Summarize the page",
    ]);
    parses(&[
        "answer",
        "What is Exa?",
        "--text",
        "--stream",
        "--output-schema",
        r#"{"type":"object"}"#,
    ]);
    assert_path(&["context", "rust async patterns"], "context");
    parses(&["context", "rust async patterns", "--tokens", "dynamic"]);
    parses(&["context", "rust async patterns", "--tokens", "1000"]);
    parses(&[
        "similar",
        "https://exa.ai",
        "--exclude-source-domain",
        "--category",
        "news",
    ]);
}

#[test]
fn parse_contents_requires_exactly_one_input_kind() {
    assert_eq!(
        parse_err(&["contents"]).kind(),
        clap::error::ErrorKind::MissingRequiredArgument
    );
    assert_eq!(
        parse_err(&["contents", "https://exa.ai", "--ids", "id1"]).kind(),
        clap::error::ErrorKind::ArgumentConflict
    );
}

#[test]
fn parse_contents_accepts_urls_only_and_ids_only() {
    let cli = parses(&[
        "contents",
        "https://exa.ai/docs",
        "https://docs.exa.ai/reference/search",
        "--text",
        "--summary-query",
        "Summarize the page",
        "--chunk-size",
        "100",
    ]);
    let Command::Contents(args) = cli.command else {
        panic!("expected contents command");
    };
    assert_eq!(
        args.urls,
        vec![
            "https://exa.ai/docs".to_string(),
            "https://docs.exa.ai/reference/search".to_string()
        ]
    );
    assert!(args.ids.is_empty());
    assert!(args.text);
    assert_eq!(args.summary_query.as_deref(), Some("Summarize the page"));
    assert_eq!(args.chunk_size, Some(100));

    let cli = parses(&["contents", "--ids", "doc_1", "doc_2"]);
    let Command::Contents(args) = cli.command else {
        panic!("expected contents command");
    };
    assert!(args.urls.is_empty());
    assert_eq!(args.ids, vec!["doc_1".to_string(), "doc_2".to_string()]);
    assert!(!args.text);
    assert_eq!(args.summary_query, None);
    assert_eq!(args.chunk_size, None);
}

#[test]
fn parse_contents_rejects_zero_chunk_size() {
    assert_eq!(
        parse_err(&["contents", "https://exa.ai", "--chunk-size", "0"]).kind(),
        clap::error::ErrorKind::ValueValidation
    );
    assert_eq!(
        parse_err(&["contents", "https://exa.ai", "--chunk-size", "101"]).kind(),
        clap::error::ErrorKind::ValueValidation
    );
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
    let ask = parses(&["ask", "What changed in AI this week?"]);
    let Command::Ask(args) = ask.command else {
        panic!("expected ask command");
    };
    assert_eq!(args.question, "What changed in AI this week?");

    let fetch = parses(&["fetch", "https://exa.ai", "https://docs.exa.ai"]);
    let Command::Fetch(args) = fetch.command else {
        panic!("expected fetch command");
    };
    assert_eq!(args.urls, ["https://exa.ai", "https://docs.exa.ai"]);
    assert_eq!(
        parse_err(&["fetch"]).kind(),
        clap::error::ErrorKind::MissingRequiredArgument
    );
}

#[test]
fn ask_dry_run_expands_to_typed_answer_request() {
    let json = run_ok_json(&[
        "ask",
        "What is Exa?",
        "--dry-run",
        "--print-request",
        "--compact",
    ]);
    let body = &json["data"]["request"]["body"];
    assert_eq!(json["command"], "answer");
    assert_eq!(json["data"]["request"]["path"], "/answer");
    assert_eq!(body["query"], "What is Exa?");
    assert_eq!(body["text"], true);
    assert_eq!(json["data"]["expandsTo"], "answer 'What is Exa?' --text");
    assert_eq!(json["data"]["expands_to"], "answer 'What is Exa?' --text");

    let quoted = run_ok_json(&[
        "ask",
        "What's Exa?",
        "--dry-run",
        "--print-request",
        "--compact",
    ]);
    assert_eq!(
        quoted["data"]["expandsTo"],
        "answer 'What'\\''s Exa?' --text"
    );
}

#[test]
fn ask_live_response_does_not_include_macro_metadata() {
    let (base_url, server) = local_json_server(
        |request_text| {
            assert!(
                request_text.starts_with("POST /answer "),
                "unexpected request:\n{request_text}"
            );
            assert!(
                request_text
                    .to_ascii_lowercase()
                    .contains("x-api-key: test-key-abcdef12"),
                "request did not include explicit test API key:\n{request_text}"
            );
            assert!(
                request_text.contains(r#""query":"What is Exa?""#)
                    && request_text.contains(r#""text":true"#),
                "ask macro did not send the typed answer body:\n{request_text}"
            );
        },
        br#"{"answer":"done","citations":[]}"#,
    );
    let output = run(&[
        "ask",
        "What is Exa?",
        "--api-key",
        "test-key-abcdef12",
        "--base-url",
        base_url.as_str(),
        "--compact",
    ]);
    server.join().expect("local test server panicked");

    assert!(
        output.status.success(),
        "expected live local ask success\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let upstream = serde_json::json!({"answer":"done","citations":[]});
    assert_eq!(json["command"], "answer");
    assert_eq!(json["data"], upstream);
    assert!(json["data"].get("expandsTo").is_none());
    assert!(json["data"].get("expands_to").is_none());
    assert_eq!(json["dataHash"], transport::data_hash(&upstream).unwrap());
}

#[test]
fn fetch_live_response_does_not_include_macro_metadata() {
    let upstream =
        br#"{"results":[{"url":"https://exa.ai","text":"ok"}],"statuses":[{"id":"https://exa.ai","status":"success"}]}"#;
    let (base_url, server) = local_json_server(
        |request_text| {
            assert!(
                request_text.starts_with("POST /contents "),
                "unexpected request:\n{request_text}"
            );
            assert!(
                request_text
                    .to_ascii_lowercase()
                    .contains("x-api-key: test-key-abcdef12"),
                "request did not include explicit test API key:\n{request_text}"
            );
            assert!(
                request_text.contains(r#""urls":["https://exa.ai"]"#)
                    && request_text.contains(r#""text":true"#)
                    && request_text.contains(r#""summary":{"query":"Summarize the page"}"#),
                "fetch macro did not send the typed contents body:\n{request_text}"
            );
        },
        upstream,
    );
    let output = run(&[
        "fetch",
        "https://exa.ai",
        "--api-key",
        "test-key-abcdef12",
        "--base-url",
        base_url.as_str(),
        "--compact",
    ]);
    server.join().expect("local test server panicked");

    assert!(
        output.status.success(),
        "expected live local fetch success\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let upstream: serde_json::Value = serde_json::from_slice(upstream).unwrap();
    assert_eq!(json["command"], "contents");
    assert_eq!(json["data"], upstream);
    assert!(json["data"].get("expandsTo").is_none());
    assert!(json["data"].get("expands_to").is_none());
    assert_eq!(json["dataHash"], transport::data_hash(&upstream).unwrap());
}

#[test]
fn fetch_dry_run_expands_to_typed_contents_request() {
    let json = run_ok_json(&[
        "fetch",
        "https://exa.ai",
        "https://docs.exa.ai/search?q=rust&sort=new",
        "--dry-run",
        "--compact",
    ]);
    let body = &json["data"]["request"]["body"];
    assert_eq!(json["command"], "contents");
    assert_eq!(json["data"]["request"]["path"], "/contents");
    assert_eq!(
        body["urls"],
        serde_json::json!([
            "https://exa.ai",
            "https://docs.exa.ai/search?q=rust&sort=new"
        ])
    );
    assert_eq!(body["text"], true);
    assert_eq!(body["summary"]["query"], "Summarize the page");
    assert_eq!(
        json["data"]["expandsTo"],
        "contents 'https://exa.ai' 'https://docs.exa.ai/search?q=rust&sort=new' --text --summary-query 'Summarize the page'"
    );
    assert_eq!(
        json["data"]["expands_to"],
        "contents 'https://exa.ai' 'https://docs.exa.ai/search?q=rust&sort=new' --text --summary-query 'Summarize the page'"
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
    assert!(cli.globals.retry_after);
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
