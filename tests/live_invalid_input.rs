use std::process::{Command, Output};

#[derive(Clone, Copy)]
enum DetailValue {
    Str(&'static str),
    Int(i64),
}

struct InvalidCase {
    name: &'static str,
    args: &'static [&'static str],
    code: &'static str,
    details: &'static [(&'static str, DetailValue)],
}

fn run(args: &[&str]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_exa-agent"));
    cmd.args(args)
        .env("EXA_API_KEY", "test-fake-key")
        .env_remove("EXA_SERVICE_KEY")
        .env_remove("EXA_ADMIN_BASE_URL")
        .env_remove("EXA_AGENT_CREDENTIALS")
        .env_remove("EXA_AGENT_CONFIG")
        .env_remove("EXA_PROFILE");
    cmd.output()
        .unwrap_or_else(|e| panic!("failed to run exa-agent {args:?}: {e}"))
}

fn assert_usage_code(args: &[&str], code: &str) -> serde_json::Value {
    let output = run(args);
    assert_eq!(
        output.status.code(),
        Some(1),
        "expected usage exit for {args:?}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.stdout.is_empty(),
        "expected no stdout for {args:?}: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr: serde_json::Value = serde_json::from_slice(&output.stderr)
        .unwrap_or_else(|e| panic!("stderr was not JSON for {args:?}: {e}"));
    assert_eq!(stderr["schema"], "exa.cli.error.v1", "{args:?}");
    assert_eq!(stderr["ok"], false, "{args:?}");
    assert_eq!(stderr["error"]["exitCode"], 1, "{args:?}");
    assert_eq!(stderr["error"]["code"], code, "{args:?}");
    stderr
}

fn assert_detail(stderr: &serde_json::Value, case: &InvalidCase, key: &str, expected: DetailValue) {
    let actual = &stderr["error"]["details"][key];
    match expected {
        DetailValue::Str(expected) => assert_eq!(actual, expected, "{} detail `{key}`", case.name),
        DetailValue::Int(expected) => assert_eq!(actual, expected, "{} detail `{key}`", case.name),
    }
}

#[test]
fn modeled_live_invalid_inputs_fail_locally() {
    use DetailValue::{Int, Str};

    let cases = [
        InvalidCase {
            name: "search wrong numResults type",
            args: &[
                "search",
                "test query",
                "--set",
                "numResults=five",
                "--compact",
            ],
            code: "invalid_field_type",
            details: &[
                ("field", Str("numResults")),
                ("flag", Str("num-results")),
                ("expected", Str("integer")),
                ("received", Str("five")),
            ],
        },
        InvalidCase {
            name: "search numResults out of range",
            args: &[
                "search",
                "test query",
                "--set",
                "numResults=500",
                "--compact",
            ],
            code: "invalid_value",
            details: &[("min", Int(1)), ("max", Int(100)), ("received", Int(500))],
        },
        InvalidCase {
            name: "answer missing query",
            args: &[
                "answer",
                "placeholder question",
                "--set",
                "query=null",
                "--compact",
            ],
            code: "missing_required_field",
            details: &[("field", Str("query")), ("flag", Str("question"))],
        },
        InvalidCase {
            name: "context wrong tokensNum type",
            args: &[
                "context",
                "test query",
                "--set",
                "tokensNum=nope",
                "--compact",
            ],
            code: "invalid_field_type",
            details: &[
                ("field", Str("tokensNum")),
                ("flag", Str("tokens")),
                ("expected", Str("integer")),
            ],
        },
        InvalidCase {
            name: "similar wrong numResults type",
            args: &[
                "similar",
                "https://example.com",
                "--set",
                "numResults=nope",
                "--compact",
            ],
            code: "invalid_field_type",
            details: &[
                ("field", Str("numResults")),
                ("flag", Str("num-results")),
                ("expected", Str("integer")),
            ],
        },
        InvalidCase {
            name: "contents wrong text type",
            args: &[
                "contents",
                "https://example.com",
                "--set",
                "text=maybe",
                "--compact",
            ],
            code: "invalid_field_type",
            details: &[
                ("field", Str("text")),
                ("flag", Str("text")),
                ("expected", Str("boolean")),
            ],
        },
        InvalidCase {
            name: "agent runs create wrong stream type",
            args: &[
                "agent",
                "runs",
                "create",
                "find eval tools",
                "--set",
                "stream=maybe",
                "--compact",
            ],
            code: "invalid_field_type",
            details: &[
                ("field", Str("stream")),
                ("flag", Str("stream")),
                ("expected", Str("boolean")),
            ],
        },
        InvalidCase {
            name: "research create missing instructions",
            args: &[
                "research",
                "create",
                "test query",
                "--set",
                "instructions=null",
                "--compact",
            ],
            code: "missing_required_field",
            details: &[("field", Str("instructions")), ("flag", Str("query"))],
        },
    ];

    for case in cases {
        let stderr = assert_usage_code(case.args, case.code);
        for (key, expected) in case.details {
            assert_detail(&stderr, &case, key, *expected);
        }
    }
}

#[test]
fn chunked_contents_invalid_input_fails_locally_before_network() {
    use DetailValue::Str;

    let case = InvalidCase {
        name: "chunked contents wrong text type",
        args: &[
            "contents",
            "https://example.com",
            "--set",
            "text=maybe",
            "--chunk-size",
            "50",
            "--compact",
        ],
        code: "invalid_field_type",
        details: &[
            ("field", Str("text")),
            ("flag", Str("text")),
            ("expected", Str("boolean")),
        ],
    };
    let stderr = assert_usage_code(case.args, case.code);
    for (key, expected) in case.details {
        assert_detail(&stderr, &case, key, *expected);
    }
}
