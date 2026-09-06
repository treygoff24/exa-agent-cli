//! Offline request and self-description contracts from the September 2026 API refresh.

use serde_json::{json, Value};
use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    let isolated = std::env::temp_dir().join(format!("exa-api-refresh-{}", std::process::id()));
    let mut command = Command::new(env!("CARGO_BIN_EXE_exa-agent"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("EXA_") {
            command.env_remove(key);
        }
    }
    command
        .args(args)
        .arg("--json")
        .env("EXA_AGENT_NO_NETWORK", "1")
        .env("EXA_AGENT_CONFIG", isolated.join("config.toml"))
        .env("EXA_AGENT_CREDENTIALS", isolated.join("credentials.json"))
        .env("EXA_AGENT_PRESETS", isolated.join("presets.toml"))
        .env(
            "EXA_AGENT_LOCAL_PRESETS",
            isolated.join("local-presets.toml"),
        )
        .env("EXA_AGENT_STATE", isolated.join("state"))
        .output()
        .expect("run exa-agent")
}

fn ok(args: &[&str]) -> Value {
    let output = run(args);
    assert!(
        output.status.success(),
        "{args:?}: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("one JSON response")
}

#[test]
fn answer_named_options_match_the_current_api() {
    for model in ["exa", "exa-pro", "exa-research", "exa-fast"] {
        let response = ok(&[
            "answer",
            "What changed?",
            "--model",
            model,
            "--system-prompt",
            "Prefer primary sources.",
            "--user-location",
            "US",
            "--dry-run",
            "--print-request",
        ]);
        assert_eq!(
            response["data"]["request"]["body"],
            json!({
                "query": "What changed?",
                "model": model,
                "systemPrompt": "Prefer primary sources.",
                "userLocation": "US"
            })
        );
    }
}

#[test]
fn answer_body_and_set_override_named_options() {
    let response = ok(&[
        "answer",
        "q",
        "--model",
        "exa",
        "--system-prompt",
        "Flag prompt",
        "--user-location",
        "US",
        "--body",
        r#"{"model":"exa-pro","systemPrompt":"Body prompt","userLocation":"CA"}"#,
        "--set",
        "model=exa-fast",
        "--dry-run",
    ]);
    let body = &response["data"]["request"]["body"];
    assert_eq!(body["model"], "exa-fast");
    assert_eq!(body["systemPrompt"], "Body prompt");
    assert_eq!(body["userLocation"], "CA");
}

#[test]
fn answer_reads_system_prompt_from_a_file() {
    let directory = std::env::temp_dir().join(format!(
        "exa-answer-prompt-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("prompt.txt");
    std::fs::write(&path, "Prefer primary sources.\n").unwrap();
    let argument = format!("@{}", path.display());
    let output = run(&["answer", "q", "--system-prompt", &argument, "--dry-run"]);
    std::fs::remove_dir_all(&directory).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        response["data"]["request"]["body"]["systemPrompt"],
        "Prefer primary sources.\n"
    );
}

#[test]
fn answer_rejects_unknown_models_from_flags_and_body() {
    for options in [
        ["--model", "imaginary-model"],
        ["--body", r#"{"model":"imaginary-model"}"#],
    ] {
        let output = run(&["answer", "q", options[0], options[1], "--dry-run"]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["error"]["code"], "invalid_value");
        let allowed = json!(["exa", "exa-pro", "exa-research", "exa-fast"]);
        if options[0] == "--model" {
            assert!(error["error"]["message"]
                .as_str()
                .unwrap()
                .contains("imaginary-model"));
            assert_eq!(error["error"]["details"]["validValues"], allowed);
        } else {
            assert_eq!(error["error"]["details"]["value"], "imaginary-model");
            assert_eq!(error["error"]["details"]["allowed"], allowed);
        }
    }
}

#[test]
fn answer_location_accepts_null_but_rejects_non_string_values() {
    let response = ok(&[
        "answer",
        "q",
        "--body",
        r#"{"userLocation":null}"#,
        "--dry-run",
    ]);
    let body = &response["data"]["request"]["body"];
    assert!(body.as_object().unwrap().contains_key("userLocation"));
    assert!(body["userLocation"].is_null());

    for location in [json!(123), json!(["US"])] {
        let output = run(&[
            "answer",
            "q",
            "--body",
            &json!({"userLocation":location}).to_string(),
            "--dry-run",
        ]);
        assert_eq!(output.status.code(), Some(1));
        assert!(output.stdout.is_empty());
        let error: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["error"]["code"], "invalid_field_type");
        assert!(
            error["error"]["message"]
                .as_str()
                .unwrap()
                .contains("userLocation")
                || error["error"]["details"]["field"] == "userLocation",
            "{error}"
        );
    }
}

#[test]
fn agent_accepts_polymarket_as_a_named_provider() {
    let response = ok(&[
        "agent",
        "runs",
        "create",
        "Research prediction markets",
        "--data-source",
        "polymarket",
        "--dry-run",
    ]);
    assert_eq!(
        response["data"]["request"]["body"]["dataSources"],
        json!([{"provider":"polymarket"}])
    );
}

fn validate(command: &str, body: &Value) -> Value {
    ok(&[
        "schema",
        "validate-input",
        command,
        "--body",
        &body.to_string(),
    ])
}

#[test]
fn monitor_schema_accepts_domain_filters_on_create_and_update() {
    for (command, body) in [
        (
            "monitor create",
            json!({
                "search": {
                    "query": "AI",
                    "includeDomains": ["exa.ai"],
                    "excludeDomains": ["example.com"]
                },
                "webhook": {"url":"https://example.com/hook"}
            }),
        ),
        (
            "monitor update",
            json!({"search":{"includeDomains":[],"excludeDomains":["example.com"]}}),
        ),
    ] {
        let result = validate(command, &body);
        assert_eq!(result["valid"], true, "{command}: {result}");
    }
}

#[test]
fn monitor_schema_accepts_dynamic_highlights_and_rejects_unknown_fields() {
    let body = json!({
        "search": {
            "query": "AI",
            "contents": {"highlights":{"dynamic":true,"verbosity":"medium"}}
        },
        "webhook": {"url":"https://example.com/hook"}
    });
    for command in ["monitor create", "monitor update"] {
        let result = validate(command, &body);
        assert_eq!(result["valid"], true, "{command}: {result}");

        let mut invalid = body.clone();
        invalid["search"]["contents"]["highlights"]["imaginaryOption"] = json!(true);
        let result = validate(command, &invalid);
        assert_eq!(result["valid"], false, "{command}: {result}");
        assert_eq!(
            result["details"]["field"],
            "search.contents.highlights.imaginaryOption"
        );
    }
}

#[test]
fn monitor_domain_flags_preserve_arrays_and_body_set_precedence() {
    for prefix in [
        vec![
            "monitor",
            "create",
            "--query",
            "AI",
            "--webhook-url",
            "https://example.com/hook",
        ],
        vec!["monitor", "update", "monitor_abc123"],
    ] {
        let mut args = prefix;
        args.extend([
            "--include-domain",
            "exa.ai",
            "--include-domain",
            "example.com",
            "--exclude-domain",
            "blocked.example",
            "--dry-run",
        ]);
        let result = ok(&args);
        let search = &result["data"]["request"]["body"]["search"];
        assert_eq!(search["includeDomains"], json!(["exa.ai", "example.com"]));
        assert_eq!(search["excludeDomains"], json!(["blocked.example"]));
        args.extend([
            "--body",
            r#"{"search":{"includeDomains":["body.example"]}}"#,
            "--set",
            r#"search.excludeDomains=[]"#,
        ]);
        let result = ok(&args);
        let search = &result["data"]["request"]["body"]["search"];
        assert_eq!(search["includeDomains"], json!(["body.example"]));
        assert_eq!(search["excludeDomains"], json!([]));
    }
    let output = run(&[
        "monitor",
        "update",
        "monitor_abc123",
        "--body",
        r#"{"search":{"includeDomains":"exa.ai"}}"#,
        "--dry-run",
    ]);
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
}

fn highlight_bodies(highlights: &Value) -> [(&'static str, Value); 4] {
    [
        (
            "search",
            json!({"query":"AI","contents":{"highlights":highlights}}),
        ),
        (
            "contents",
            json!({"ids":["https://exa.ai"],"highlights":highlights}),
        ),
        (
            "monitor create",
            json!({
                "search":{"query":"AI","contents":{"highlights":highlights}},
                "webhook":{"url":"https://example.com/hook"}
            }),
        ),
        (
            "monitor update",
            json!({"search":{"contents":{"highlights":highlights}}}),
        ),
    ]
}

fn highlight_request(command: &str, body: &Value, beta: bool) -> Output {
    let body = body.to_string();
    let mut args: Vec<&str> = command.split_whitespace().collect();
    match command {
        "search" => args.push("AI"),
        "contents" => args.extend(["--ids", "https://exa.ai"]),
        "monitor update" => args.push("demo"),
        _ => {}
    }
    args.extend(["--body", &body, "--dry-run"]);
    if beta {
        args.extend(["--beta", "dynamic-highlights-2026-08-28"]);
    }
    run(&args)
}

#[test]
fn highlights_json_and_file_flags_preserve_dynamic_configuration() {
    let directory = std::env::temp_dir().join(format!(
        "exa-highlights-input-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("highlights.json");
    let highlights = json!({"dynamic":true,"verbosity":"medium"});
    let inline = highlights.to_string();
    std::fs::write(&path, &inline).unwrap();
    let file = format!("@{}", path.display());
    let mut responses = Vec::new();
    for command in ["search", "contents"] {
        for value in [&inline, &file] {
            responses.push((
                command,
                run(&[
                    command,
                    if command == "search" {
                        "AI"
                    } else {
                        "https://exa.ai"
                    },
                    "--highlights",
                    value,
                    "--beta",
                    "dynamic-highlights-2026-08-28",
                    "--dry-run",
                ]),
            ));
        }
    }
    std::fs::remove_dir_all(&directory).unwrap();
    for (command, output) in responses {
        assert!(
            output.status.success(),
            "{command}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        let body = &response["data"]["request"]["body"];
        let options = if command == "search" {
            &body["contents"]
        } else {
            body
        };
        assert_eq!(options["highlights"], highlights);
    }
}

#[test]
fn highlights_schema_checks_body_shape_without_requiring_beta_headers() {
    for highlights in [
        json!({"dynamic":true,"verbosity":"medium"}),
        json!({"dynamic":null,"verbosity":null}),
    ] {
        for (command, body) in highlight_bodies(&highlights) {
            let result = validate(command, &body);
            assert_eq!(result["valid"], true, "{command}: {result}");
            if highlights["dynamic"].is_null() {
                let output = highlight_request(command, &body, false);
                assert!(
                    output.status.success(),
                    "{command}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}

#[test]
fn highlights_invalid_types_and_conflicts_fail_schema_and_request_validation() {
    for (highlights, error_code) in [
        (json!({"dynamic":"true"}), "invalid_field_type"),
        (json!({"verbosity":"ultra"}), "invalid_field_type"),
        (
            json!({"dynamic":true,"maxCharacters":1000}),
            "invalid_flag_combination",
        ),
        (
            json!({"verbosity":"medium","numSentences":2}),
            "invalid_flag_combination",
        ),
        (
            json!({"verbosity":"medium","maxCharacters":1000}),
            "invalid_flag_combination",
        ),
    ] {
        for (command, body) in highlight_bodies(&highlights) {
            let result = validate(command, &body);
            assert_eq!(result["valid"], false, "{command}, {highlights}: {result}");

            let output = highlight_request(command, &body, true);
            assert_eq!(output.status.code(), Some(1), "{command}, {highlights}");
            assert!(output.stdout.is_empty());
            let error: Value = serde_json::from_slice(&output.stderr).unwrap();
            assert_eq!(error["error"]["code"], error_code, "{error}");
        }
    }
}

#[test]
fn highlights_beta_features_require_explicit_opt_in_before_dispatch() {
    for highlights in [json!({"dynamic":false}), json!({"verbosity":"low"})] {
        for (command, body) in highlight_bodies(&highlights) {
            let output = highlight_request(command, &body, false);
            assert_eq!(output.status.code(), Some(1), "{command}, {highlights}");
            assert!(output.stdout.is_empty());
            let error: Value = serde_json::from_slice(&output.stderr).unwrap();
            assert_eq!(error["error"]["code"], "invalid_flag_combination");
            assert!(
                error.to_string().contains("dynamic-highlights-2026-08-28"),
                "{error}"
            );

            let output = highlight_request(command, &body, true);
            assert!(
                output.status.success(),
                "{command}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
